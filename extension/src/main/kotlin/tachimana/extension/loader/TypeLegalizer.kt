package tachimana.extension.loader

import io.github.oshai.kotlinlogging.KotlinLogging
import org.objectweb.asm.Opcodes
import org.objectweb.asm.Type
import org.objectweb.asm.tree.AbstractInsnNode
import org.objectweb.asm.tree.ClassNode
import org.objectweb.asm.tree.FieldInsnNode
import org.objectweb.asm.tree.InsnList
import org.objectweb.asm.tree.InsnNode
import org.objectweb.asm.tree.MethodInsnNode
import org.objectweb.asm.tree.MethodNode
import org.objectweb.asm.tree.TypeInsnNode
import org.objectweb.asm.tree.VarInsnNode
import org.objectweb.asm.tree.analysis.Analyzer
import org.objectweb.asm.tree.analysis.AnalyzerException
import org.objectweb.asm.tree.analysis.BasicValue
import org.objectweb.asm.tree.analysis.Frame
import org.objectweb.asm.tree.analysis.SimpleVerifier
import org.objectweb.asm.tree.analysis.SourceInterpreter
import org.objectweb.asm.tree.analysis.SourceValue

/**
 * Makes dex2jar output verifiable and correct on the JVM. Two passes over every method:
 *
 * **1. Allocation retyping.** R8 inlines trivial constructors: the DEX contains
 * `new-instance X` followed by `invoke-direct Object.<init>` and no `X.<init>` exists at all.
 * dex2jar cannot express that on the JVM and emits `new java/lang/Object` — allocating the
 * wrong class, which then breaks every field access on the value. This pass flows each
 * `new Object` to its use sites; when they demand a concrete class X from the jar, the `NEW`
 * is retyped to X, its paired `Object.<init>` call is redirected to X, and the no-arg
 * constructor R8 stripped is synthesized back into X.
 *
 * **2. Checkcast insertion.** DEX registers are untyped, so ART accepts code where a register
 * statically merges to a broad type yet is used where a narrower one is required. dex2jar
 * translates such code 1:1 and the JVM verifier rejects it ("Bad type on operand stack").
 * This pass runs ASM's dataflow analysis with a hierarchy-aware, non-classloading verifier and
 * inserts `checkcast` at the use sites that need one (a checkcast on a value that already has
 * the right runtime type is a no-op, so over-inserting is safe). Covered: putfield/putstatic
 * value, putfield/getfield receiver, areturn, athrow, invoke last-argument and no-arg receiver.
 * Deeper operand mismatches are only logged; they have not been observed in practice.
 */
internal object TypeLegalizer {
    private val logger = KotlinLogging.logger {}

    fun legalize(
        nodes: List<ClassNode>,
        hierarchy: ClassHierarchy,
        dexAllocations: DexAllocationIndex? = null,
    ) {
        val nodesByName = nodes.associateBy { it.name }
        nodes.forEach { node ->
            node.methods.toList().forEach { method ->
                if (method.instructions.size() > 0) {
                    // dex2jar sometimes under-declares maxStack, which would abort the
                    // analysis; give it headroom — the ClassWriter recomputes real maxs anyway.
                    method.maxStack += 16
                    retypeInlinedAllocations(node, method, nodesByName, hierarchy, dexAllocations)
                    insertCheckcasts(node.name, method, hierarchy)
                }
            }
        }
    }

    // ------------------------------------------------------------------ pass 1: allocations

    /** [SourceInterpreter] whose copy operations preserve value identity, so a value's source
     *  set stays the set of instructions that *created* it, across loads/stores/dups. */
    private class OriginInterpreter : SourceInterpreter(Opcodes.ASM9) {
        override fun copyOperation(
            insn: AbstractInsnNode,
            value: SourceValue,
        ): SourceValue = value
    }

    private fun retypeInlinedAllocations(
        node: ClassNode,
        method: MethodNode,
        nodesByName: Map<String, ClassNode>,
        hierarchy: ClassHierarchy,
        dexAllocations: DexAllocationIndex?,
    ) {
        // Cheap pre-check: only analyze methods that allocate, or constructors that invokespecial
        // an <init> that is neither their own class's nor their direct superclass's (R8's inlined
        // super-constructor chain, e.g. an enum subclass calling Enum.<init> directly).
        val insnArray = method.instructions.toArray()
        val allocates = insnArray.any { it is TypeInsnNode && it.opcode == Opcodes.NEW }
        val skipsSuperCtor =
            method.name == "<init>" &&
                node.superName != null &&
                insnArray.any {
                    it is MethodInsnNode &&
                        it.opcode == Opcodes.INVOKESPECIAL &&
                        it.name == "<init>" &&
                        it.owner != node.name &&
                        it.owner != node.superName
                }
        if (!allocates && !skipsSuperCtor) return

        val frames: Array<Frame<SourceValue>?> =
            try {
                Analyzer(OriginInterpreter()).analyze(node.name, method)
            } catch (e: AnalyzerException) {
                logger.warn { "origin analysis failed for ${node.name}.${method.name}${method.desc}: $e" }
                return
            }

        val demands = HashMap<TypeInsnNode, MutableSet<String>>()
        val initCalls = HashMap<TypeInsnNode, MutableSet<MethodInsnNode>>()
        val superCallsToFix = ArrayList<MethodInsnNode>()
        val insns = insnArray

        insns.forEachIndexed { i, insn ->
            val frame = frames.getOrNull(i) ?: return@forEachIndexed

            fun stackSources(depthFromTop: Int): Set<AbstractInsnNode>? {
                val idx = frame.stackSize - 1 - depthFromTop
                if (idx < 0) return null
                return frame.getStack(idx)?.insns
            }

            /** The `NEW` instructions this operand came from, or null if it (also) came from
             *  anything else. */
            fun allocations(depthFromTop: Int): List<TypeInsnNode>? {
                val sources = stackSources(depthFromTop) ?: return null
                if (sources.isEmpty()) return null
                val news =
                    sources.map {
                        val t = it as? TypeInsnNode ?: return null
                        if (t.opcode != Opcodes.NEW) return null
                        t
                    }
                return news
            }

            fun demand(
                type: String,
                depthFromTop: Int,
            ) {
                if (type == ClassHierarchy.OBJECT || type.startsWith("[")) return
                allocations(depthFromTop)?.forEach { demands.getOrPut(it) { HashSet() } += type }
            }

            /** Demand the declared class type of a value position (ignores primitives/arrays). */
            fun demandType(
                type: Type,
                depthFromTop: Int,
            ) {
                if (type.sort == Type.OBJECT) demand(type.internalName, depthFromTop)
            }

            when {
                insn is FieldInsnNode && insn.opcode == Opcodes.PUTFIELD -> {
                    demand(insn.owner, 1)
                    demandType(Type.getType(insn.desc), 0)
                }
                insn is FieldInsnNode && insn.opcode == Opcodes.PUTSTATIC -> demandType(Type.getType(insn.desc), 0)
                insn is FieldInsnNode && insn.opcode == Opcodes.GETFIELD -> demand(insn.owner, 0)
                insn.opcode == Opcodes.ARETURN -> demandType(Type.getReturnType(method.desc), 0)
                // R8 emits its own checkcast after a mistyped allocation (enum pattern); treat
                // the cast target as a demand so it reaches the NEW through the source chain.
                insn is TypeInsnNode && insn.opcode == Opcodes.CHECKCAST -> demand(insn.desc, 0)
                // Storing into an array whose provenance is a known anewarray demands the
                // component type (the enum pattern fills a `values()` array this way).
                insn.opcode == Opcodes.AASTORE -> {
                    val arraySources = stackSources(2)
                    if (!arraySources.isNullOrEmpty() &&
                        arraySources.all { it is TypeInsnNode && it.opcode == Opcodes.ANEWARRAY }
                    ) {
                        arraySources
                            .map { (it as TypeInsnNode).desc }
                            .toSet()
                            .singleOrNull()
                            ?.let { component -> demand(component, 0) }
                    }
                }
                insn is MethodInsnNode -> {
                    val args = Type.getArgumentTypes(insn.desc)
                    args.forEachIndexed { a, arg -> demandType(arg, args.size - 1 - a) }
                    if (insn.opcode != Opcodes.INVOKESTATIC) {
                        val depth = args.size
                        if (insn.name == "<init>") {
                            val receiverAllocations = allocations(depth)
                            if (receiverAllocations != null) {
                                receiverAllocations.forEach { initCalls.getOrPut(it) { HashSet() } += insn }
                            } else if (skipsSuperCtor &&
                                method.name == "<init>" &&
                                insn.owner != node.name &&
                                insn.owner != node.superName &&
                                stackSources(depth)?.isEmpty() == true
                            ) {
                                // Receiver produced by no instruction = a parameter = `this` in
                                // a constructor: R8 inlined the (trivial) super-constructor
                                // chain and calls an ancestor's <init> directly. Illegal on the
                                // JVM — redirect to the direct superclass's constructor.
                                superCallsToFix += insn
                            }
                        } else {
                            demand(insn.owner, depth)
                        }
                    }
                }
            }
        }

        // Resolve a retype target per allocation. Ground truth first: the i-th NEW in the
        // translated method corresponds to the i-th `new-instance` in the DEX, which still names
        // the real class dex2jar discarded. Demand inference remains as fallback (raw-DEX
        // installs, alignment failures).
        val targets = LinkedHashMap<TypeInsnNode, String>()
        val jarNews = insns.filterIsInstance<TypeInsnNode>().filter { it.opcode == Opcodes.NEW }
        val dexNews = dexAllocations?.allocations(node.name, method.name, method.desc)
        if (dexNews != null) {
            val aligned =
                dexNews.size == jarNews.size &&
                    jarNews.zip(dexNews).all { (n, t) -> n.desc == t || hierarchy.isAssignableFrom(n.desc, t) }
            if (aligned) {
                jarNews.zip(dexNews).forEach { (n, t) -> if (n.desc != t) targets[n] = t }
            } else {
                logger.warn {
                    "DEX allocation alignment failed for ${node.name}.${method.name} " +
                        "(${dexNews.size} dex vs ${jarNews.size} jar); using demand inference"
                }
            }
        }

        demands.forEach { (allocation, types) ->
            if (allocation in targets) return@forEach
            // The common case: a correctly-typed allocation whose demands are all supertypes.
            if (types.all { hierarchy.isAssignableFrom(it, allocation.desc) }) return@forEach
            // Demands may legitimately mix a class with interfaces it implements (e.g. a
            // kotlinx.serialization $serializer stored into a GeneratedSerializer-typed slot);
            // the answer is the most specific demand — the one assignable to all others.
            val target = types.singleOrNull { t -> types.all { hierarchy.isAssignableFrom(it, t) } }
            if (target == null) {
                logger.warn {
                    "cannot retype allocation in ${node.name}.${method.name}: conflicting demands $types"
                }
                return@forEach
            }
            if (target == allocation.desc) return@forEach // demands match the allocation already
            if (!hierarchy.isAssignableFrom(allocation.desc, target)) {
                logger.warn {
                    "cannot retype allocation of ${allocation.desc} in ${node.name}.${method.name}: " +
                        "demand $target is not a subclass"
                }
                return@forEach
            }
            targets[allocation] = target
        }

        targets.forEach { (allocation, target) ->
            val inits = initCalls[allocation].orEmpty()
            // Only rewrite the clean `new; dup; invokespecial <init>` shape: every init call
            // must belong to this allocation alone.
            if (inits.any { init -> initCalls.any { (other, calls) -> other !== allocation && init in calls } }) {
                logger.warn { "cannot retype allocation in ${node.name}.${method.name}: shared <init> call" }
                return@forEach
            }
            if (inits.any { !ensureConstructor(target, it.desc, nodesByName, hierarchy) }) {
                logger.warn {
                    "cannot retype allocation in ${node.name}.${method.name}: no way to construct $target"
                }
                return@forEach
            }
            logger.debug { "retyping ${allocation.desc} allocation in ${node.name}.${method.name} to $target" }
            allocation.desc = target
            inits.forEach { it.owner = target }
        }

        if (superCallsToFix.isNotEmpty()) {
            val superName = node.superName
            superCallsToFix.forEach { call ->
                if (superName != null && ensureConstructor(superName, call.desc, nodesByName, hierarchy)) {
                    logger.debug { "redirecting ${call.owner}.<init> to $superName.<init> in ${node.name}.<init>" }
                    call.owner = superName
                } else {
                    logger.warn {
                        "cannot redirect skipped super constructor ${call.owner}.<init>${call.desc} " +
                            "in ${node.name}.<init> (super $superName)"
                    }
                }
            }
        }
    }

    /** Make sure [name] has a callable `<init>` with descriptor [desc], synthesizing the
     *  argument-forwarding constructor R8 stripped if needed (only possible for jar classes).
     *  Returns false when that isn't possible. */
    private fun ensureConstructor(
        name: String,
        desc: String,
        nodesByName: Map<String, ClassNode>,
        hierarchy: ClassHierarchy,
    ): Boolean {
        val node = nodesByName[name] ?: return hierarchy.hasConstructor(name, desc) // host class: can't synthesize
        if (node.methods.any { it.name == "<init>" && it.desc == desc }) return true
        // R8 only strips constructors that do nothing but call super — recreate exactly that.
        val superName = node.superName ?: ClassHierarchy.OBJECT
        if (!ensureConstructor(superName, desc, nodesByName, hierarchy)) return false
        val ctor = MethodNode(Opcodes.ACC_PUBLIC or Opcodes.ACC_SYNTHETIC, "<init>", desc, null, null)
        ctor.instructions.add(VarInsnNode(Opcodes.ALOAD, 0))
        var slot = 1
        Type.getArgumentTypes(desc).forEach { arg ->
            ctor.instructions.add(VarInsnNode(arg.getOpcode(Opcodes.ILOAD), slot))
            slot += arg.size
        }
        ctor.instructions.add(MethodInsnNode(Opcodes.INVOKESPECIAL, superName, "<init>", desc, false))
        ctor.instructions.add(InsnNode(Opcodes.RETURN))
        ctor.maxStack = slot
        ctor.maxLocals = slot
        node.methods.add(ctor)
        logger.debug { "synthesized constructor $name.<init>$desc" }
        return true
    }

    // ------------------------------------------------------------------ pass 2: checkcasts

    private fun insertCheckcasts(
        owner: String,
        method: MethodNode,
        hierarchy: ClassHierarchy,
    ) {
        val frames: Array<Frame<BasicValue>?> =
            try {
                Analyzer(HierarchyVerifier(hierarchy)).analyze(owner, method)
            } catch (e: AnalyzerException) {
                logger.warn { "type analysis failed for $owner.${method.name}${method.desc}: $e" }
                return
            }

        // Snapshot: insertions below must not disturb frame/instruction alignment.
        val insns = method.instructions.toArray()
        insns.forEachIndexed { i, insn ->
            val frame = frames.getOrNull(i) ?: return@forEachIndexed

            fun stackValue(depthFromTop: Int): Type? {
                val idx = frame.stackSize - 1 - depthFromTop
                if (idx < 0) return null
                val t = frame.getStack(idx)?.type ?: return null
                return if (t.sort == Type.OBJECT || t.sort == Type.ARRAY) t else null
            }

            fun needsCast(
                expected: Type,
                depthFromTop: Int,
            ): Boolean {
                if (expected.sort != Type.OBJECT && expected.sort != Type.ARRAY) return false
                val actual = stackValue(depthFromTop) ?: return false
                if (actual.sort == Type.OBJECT && actual.internalName == "null") return false
                return !isAssignable(hierarchy, expected, actual)
            }

            fun castTop(expected: Type) {
                method.instructions.insertBefore(insn, TypeInsnNode(Opcodes.CHECKCAST, expected.internalName))
            }

            when {
                insn.opcode == Opcodes.PUTFIELD || insn.opcode == Opcodes.PUTSTATIC -> {
                    val field = insn as FieldInsnNode
                    val valueType = Type.getType(field.desc)
                    if (insn.opcode == Opcodes.PUTFIELD) {
                        val ownerType = Type.getObjectType(field.owner)
                        // Receiver sits one value below the stored value.
                        if (needsCast(ownerType, 1)) castBelowTop(method.instructions, insn, ownerType, valueType.size)
                    }
                    if (needsCast(valueType, 0)) castTop(valueType)
                }
                insn.opcode == Opcodes.GETFIELD -> {
                    val ownerType = Type.getObjectType((insn as FieldInsnNode).owner)
                    if (needsCast(ownerType, 0)) castTop(ownerType)
                }
                insn.opcode == Opcodes.ARETURN -> {
                    val returnType = Type.getReturnType(method.desc)
                    if (needsCast(returnType, 0)) castTop(returnType)
                }
                insn.opcode == Opcodes.ATHROW -> {
                    val throwable = Type.getObjectType("java/lang/Throwable")
                    if (needsCast(throwable, 0)) castTop(throwable)
                }
                insn is MethodInsnNode -> {
                    val args = Type.getArgumentTypes(insn.desc)
                    if (args.isNotEmpty() && needsCast(args.last(), 0)) castTop(args.last())
                    if (insn.opcode != Opcodes.INVOKESTATIC && insn.name != "<init>") {
                        val receiver = Type.getObjectType(insn.owner)
                        val depth = args.size // one analysis stack entry per arg regardless of slot size
                        if (depth == 0) {
                            if (needsCast(receiver, 0)) castTop(receiver)
                        } else if (needsCast(receiver, depth)) {
                            logger.warn {
                                "unfixed receiver type mismatch at $owner.${method.name}${method.desc} " +
                                    "calling ${insn.owner}.${insn.name} (receiver $depth below top)"
                            }
                        }
                    }
                }
            }
        }
    }

    /**
     * Cast the value one analysis entry below the top of stack ([slotsAbove] JVM slots above it)
     * to [expected], leaving the stack otherwise untouched.
     */
    private fun castBelowTop(
        instructions: InsnList,
        before: AbstractInsnNode,
        expected: Type,
        slotsAbove: Int,
    ) {
        val cast = TypeInsnNode(Opcodes.CHECKCAST, expected.internalName)
        if (slotsAbove == 1) {
            // [ref, v] -> [v, ref] -> [v, ref'] -> [ref', v]
            instructions.insertBefore(before, InsnNode(Opcodes.SWAP))
            instructions.insertBefore(before, cast)
            instructions.insertBefore(before, InsnNode(Opcodes.SWAP))
        } else {
            // Two-slot value (long/double):
            // [ref, v] -DUP2_X1-> [v, ref, v] -POP2-> [v, ref] -cast-> [v, ref']
            //          -DUP_X2->  [ref', v, ref'] -POP-> [ref', v]
            instructions.insertBefore(before, InsnNode(Opcodes.DUP2_X1))
            instructions.insertBefore(before, InsnNode(Opcodes.POP2))
            instructions.insertBefore(before, cast)
            instructions.insertBefore(before, InsnNode(Opcodes.DUP_X2))
            instructions.insertBefore(before, InsnNode(Opcodes.POP))
        }
    }

    /** [Type]-level assignability, arrays included. False negatives just add a harmless cast. */
    private fun isAssignable(
        hierarchy: ClassHierarchy,
        target: Type,
        source: Type,
    ): Boolean =
        when {
            target == source -> true
            source.sort == Type.OBJECT && source.internalName == "null" -> true
            target.sort == Type.OBJECT && target.internalName == ClassHierarchy.OBJECT ->
                source.sort == Type.OBJECT || source.sort == Type.ARRAY
            target.sort == Type.ARRAY ->
                source.sort == Type.ARRAY &&
                    target.dimensions == source.dimensions &&
                    target.elementType.sort == Type.OBJECT &&
                    source.elementType.sort == Type.OBJECT &&
                    hierarchy.isAssignableFrom(target.elementType.internalName, source.elementType.internalName)
            target.sort == Type.OBJECT ->
                when (source.sort) {
                    Type.OBJECT -> hierarchy.isAssignableFrom(target.internalName, source.internalName)
                    // Arrays only implement Cloneable and Serializable.
                    Type.ARRAY ->
                        target.internalName == "java/lang/Cloneable" ||
                            target.internalName == "java/io/Serializable"
                    else -> false
                }
            else -> target.sort == source.sort
        }

    /** [SimpleVerifier] that resolves through [ClassHierarchy] instead of loading classes. */
    private class HierarchyVerifier(
        private val hierarchy: ClassHierarchy,
    ) : SimpleVerifier(Opcodes.ASM9, null, null, null, false) {
        override fun isInterface(type: Type): Boolean = type.sort == Type.OBJECT && hierarchy.isInterface(type.internalName)

        override fun getSuperClass(type: Type): Type? =
            when {
                type.sort == Type.ARRAY -> Type.getObjectType(ClassHierarchy.OBJECT)
                type.sort == Type.OBJECT && type.internalName != ClassHierarchy.OBJECT ->
                    hierarchy.superOf(type.internalName)?.let { Type.getObjectType(it) }
                else -> null
            }

        override fun isAssignableFrom(
            type1: Type,
            type2: Type,
        ): Boolean = isAssignable(hierarchy, type1, type2)

        // Two reasons to replace SimpleVerifier's implementation: it falls back to getClass()
        // (which classloads), and it *rejects* ill-typed reference use sites — but tolerating
        // those is the whole point of this pass, since the legalizer patches them with
        // checkcasts after analysis. So: any reference is acceptable where a reference is
        // expected; primitives keep the strict rule (a cast cannot fix those).
        override fun isSubTypeOf(
            value: BasicValue,
            expected: BasicValue,
        ): Boolean {
            val expectedType = expected.type ?: return false
            val type = value.type ?: return false
            return when (expectedType.sort) {
                Type.INT, Type.FLOAT, Type.LONG, Type.DOUBLE -> type == expectedType
                Type.ARRAY, Type.OBJECT ->
                    type.descriptor == "Lnull;" || type.sort == Type.OBJECT || type.sort == Type.ARRAY
                else -> false
            }
        }
    }
}
