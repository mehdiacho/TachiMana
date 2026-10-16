package tachimana.extension.loader

import com.googlecode.d2j.node.DexFileNode
import com.googlecode.d2j.node.insn.TypeStmtNode
import com.googlecode.d2j.reader.MultiDexFileReader
import com.googlecode.d2j.reader.Op
import io.github.oshai.kotlinlogging.KotlinLogging

/**
 * Ground truth for [TypeLegalizer]'s allocation retyping: the ordered `new-instance` types of
 * every DEX method. dex2jar types an allocation by the `<init>` it sees called on it, so when R8
 * has constructor-inlined a class (calling `Object.<init>`/`Enum.<init>` directly) the real
 * allocated class survives only here, in the original DEX.
 */
internal class DexAllocationIndex private constructor(
    private val byMethod: Map<String, List<String>>,
) {
    /** Ordered `new-instance` internal names for a method, or null if unknown. */
    fun allocations(
        owner: String,
        name: String,
        desc: String,
    ): List<String>? = byMethod[key(owner, name, desc)]

    companion object {
        private val logger = KotlinLogging.logger {}

        private fun key(
            owner: String,
            name: String,
            desc: String,
        ) = "$owner.$name$desc"

        private fun String.dexToInternal(): String = removePrefix("L").removeSuffix(";")

        /** [bytes] is an APK or raw DEX, same input [PackageTools.dex2jar] takes. */
        fun read(bytes: ByteArray): DexAllocationIndex? =
            try {
                val file = DexFileNode()
                MultiDexFileReader.open(bytes).accept(file)
                val map = HashMap<String, List<String>>()
                file.clzs.orEmpty().forEach { clz ->
                    val owner = clz.className.dexToInternal()
                    clz.methods.orEmpty().forEach { m ->
                        val news =
                            m.codeNode?.stmts.orEmpty()
                                .filterIsInstance<TypeStmtNode>()
                                .filter { it.op == Op.NEW_INSTANCE }
                                .map { it.type.dexToInternal() }
                        if (news.isNotEmpty()) {
                            map[key(owner, m.method.name, m.method.desc)] = news
                        }
                    }
                }
                DexAllocationIndex(map)
            } catch (e: Exception) {
                logger.warn(e) { "could not index DEX allocations; falling back to inference" }
                null
            }
    }
}
