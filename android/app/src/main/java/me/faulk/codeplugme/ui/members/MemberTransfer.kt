package me.faulk.codeplugme.ui.members

import me.faulk.codeplugme.ui.common.formatSlot

/** One selectable member: its zero-based slot index and a human label. */
data class MemberCandidate(val index: Int, val label: String)

/**
 * The list math behind membership editing, kept free of view state so the rules
 * can be tested without standing up a screen.
 *
 * Ported unchanged from the macOS `MemberTransfer`, because the rules are about
 * the data and not about the platform.
 */
object MemberTransfer {

    /**
     * Candidates not already in the membership list.
     *
     * Filtering members out of the picker is what stops a member being added
     * twice, which the radio would treat as a duplicate entry.
     */
    fun available(candidates: List<MemberCandidate>, members: List<Int>): List<MemberCandidate> {
        val taken = members.toSet()
        return candidates.filter { it.index !in taken }
    }

    /**
     * The membership list as candidates, in membership order.
     *
     * A member whose record has since been deleted still gets a row, so it can
     * be removed rather than being stranded in the zone invisibly.
     */
    fun memberItems(candidates: List<MemberCandidate>, members: List<Int>): List<MemberCandidate> =
        members.map { index ->
            candidates.firstOrNull { it.index == index }
                ?: MemberCandidate(index, "#${formatSlot(index)} (missing)")
        }

    /**
     * [members] with [selection] appended, in candidate order rather than the
     * arbitrary order of the selection set, so a multi-select add lands
     * predictably.
     */
    fun adding(
        selection: Set<Int>,
        candidates: List<MemberCandidate>,
        members: List<Int>,
    ): List<Int> = members + available(candidates, members)
        .filter { it.index in selection }
        .map { it.index }

    /** [members] with [selection] removed, preserving the order of the rest. */
    fun removing(selection: Set<Int>, members: List<Int>): List<Int> =
        members.filter { it !in selection }

    /**
     * [members] with the entry at [from] moved to [to], for reordering.
     *
     * Order is not cosmetic: a zone's channel order is the order the radio steps
     * through them with the up and down keys.
     */
    fun reordering(members: List<Int>, from: Int, to: Int): List<Int> {
        if (from !in members.indices || to !in members.indices || from == to) return members
        val out = members.toMutableList()
        out.add(to, out.removeAt(from))
        return out
    }
}
