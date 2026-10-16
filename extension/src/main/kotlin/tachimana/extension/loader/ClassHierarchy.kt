package tachimana.extension.loader

import org.objectweb.asm.ClassReader
import org.objectweb.asm.Opcodes
import java.nio.file.Path

/**
 * Superclass/interface lookup over a converted extension jar plus the host classpath, built for
 * ASM's frame computation and the [TypeLegalizer]. It never links or initializes classes:
 * extension classes are read from the jar's bytes (collected before transformation starts),
 * everything else (extensions-lib ABI, OkHttp, android-compat) is resolved by reading the raw
 * `.class` resource off the host classpath.
 */
internal class ClassHierarchy(
    jarClasses: List<Pair<Path, ByteArray>>,
) {
    private class Info(
        val superName: String?,
        val interfaces: List<String>,
        val isInterface: Boolean,
        val ctorDescs: Set<String>,
    )

    private val cache = HashMap<String, Info?>()

    init {
        jarClasses.forEach { (_, bytes) ->
            val cr = ClassReader(bytes)
            cache[cr.className] = infoOf(cr)
        }
    }

    private fun infoOf(cr: ClassReader): Info {
        val ctorDescs = HashSet<String>()
        cr.accept(
            object : org.objectweb.asm.ClassVisitor(Opcodes.ASM9) {
                override fun visitMethod(
                    access: Int,
                    name: String?,
                    descriptor: String?,
                    signature: String?,
                    exceptions: Array<out String>?,
                ): org.objectweb.asm.MethodVisitor? {
                    if (name == "<init>" && descriptor != null) ctorDescs += descriptor
                    return null
                }
            },
            ClassReader.SKIP_CODE or ClassReader.SKIP_DEBUG or ClassReader.SKIP_FRAMES,
        )
        return Info(
            cr.superName,
            cr.interfaces.orEmpty().filterNotNull(),
            cr.access and Opcodes.ACC_INTERFACE != 0,
            ctorDescs,
        )
    }

    private fun info(name: String): Info? =
        cache.getOrPut(name) {
            javaClass.classLoader.getResourceAsStream("$name.class")?.use { infoOf(ClassReader(it)) }
        }

    fun isInterface(name: String): Boolean = info(name)?.isInterface ?: false

    fun superOf(name: String): String? = info(name)?.superName

    fun hasConstructor(
        name: String,
        desc: String,
    ): Boolean = info(name)?.ctorDescs?.contains(desc) ?: false

    /**
     * Whether a value of class/interface [source] can be assigned to [target] (both internal
     * names, neither an array). Walks superclasses and interfaces transitively; unresolvable
     * classes make this return false, which callers treat as "insert a checkcast" — safe, since
     * a checkcast on an actually-assignable value is a runtime no-op.
     */
    fun isAssignableFrom(
        target: String,
        source: String,
    ): Boolean {
        if (target == OBJECT || target == source) return true
        val seen = HashSet<String>()
        val queue = ArrayDeque<String>()
        queue += source
        while (queue.isNotEmpty()) {
            val current = queue.removeFirst()
            if (current == target) return true
            if (!seen.add(current)) continue
            val i = info(current) ?: continue
            i.superName?.let { queue += it }
            queue += i.interfaces
        }
        return false
    }

    /** Common superclass for [org.objectweb.asm.ClassWriter.COMPUTE_FRAMES]. */
    fun commonSuperClass(
        type1: String,
        type2: String,
    ): String {
        if (type1 == type2) return type1
        val i1 = info(type1)
        val i2 = info(type2)
        if (i1 == null || i2 == null || i1.isInterface || i2.isInterface) return OBJECT
        val ancestors = HashSet<String>()
        var t: String? = type1
        while (t != null) {
            ancestors += t
            t = info(t)?.superName
        }
        t = type2
        while (t != null) {
            if (t in ancestors) return t
            t = info(t)?.superName
        }
        return OBJECT
    }

    companion object {
        const val OBJECT = "java/lang/Object"
    }
}
