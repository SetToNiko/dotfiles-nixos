package studio.tournament

import java.io.File
import kotlin.random.Random

/** One font in a tournament, and how far it got. */
data class Entrant(val family: String, var round: Int = 0)

sealed interface Match {
    data class Pair(val left: Entrant, val right: Entrant) : Match
    data class Bye(val entrant: Entrant) : Match
}

class Tournament(fonts: List<String>, seed: Long = System.nanoTime()) {
    private var field: List<Entrant> = fonts.map(::Entrant).shuffled(Random(seed))
    private val history = mutableListOf<List<Match>>()

    val finished: Boolean get() = field.size <= 1
    val winner: Entrant? get() = field.singleOrNull()

    init {
        require(fonts.size >= 2) { "a tournament needs at least two fonts, got ${fonts.size}" }
    }

    // Pairs the fonts left; an odd one out gets a bye into the next round.
    fun nextRound(): List<Match> {
        check(!finished) { "the tournament is over" }
        val matches = field.chunked(2).map { pair ->
            when (pair.size) {
                2 -> Match.Pair(pair[0], pair[1])
                else -> Match.Bye(pair[0])
            }
        }
        history += matches
        return matches
    }

    fun decide(picks: Map<Match.Pair, Entrant>) {
        val round = history.size
        field = history.last().map { m ->
            when (m) {
                is Match.Bye -> m.entrant
                is Match.Pair -> picks[m] ?: error("no pick for ${m.left.family} vs ${m.right.family}")
            }
        }.onEach { it.round = round }
    }

    fun save(file: File) = file.writeText(
        history.flatten().joinToString("\n") { m ->
            when (m) {
                is Match.Pair -> "${m.left.family} vs ${m.right.family}"
                is Match.Bye -> "${m.entrant.family} (bye)"
            }
        }
    )
}

fun main() {
    val t = Tournament(listOf("Iosevka", "JetBrains Mono", "Fira Code", "Hack", "Monaspace Neon"), 42L)
    while (!t.finished) {
        val picks = t.nextRound().filterIsInstance<Match.Pair>().associateWith { it.left }
        t.decide(picks)
    }
    val name = t.winner?.family ?: "nobody"
    println("winner: $name (seed 0x2a, ${t.winner?.round} rounds)")
}
