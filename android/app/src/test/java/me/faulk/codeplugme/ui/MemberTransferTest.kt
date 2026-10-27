package me.faulk.codeplugme.ui

import me.faulk.codeplugme.ui.members.MemberCandidate
import me.faulk.codeplugme.ui.members.MemberTransfer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The membership list math, ported unchanged from the macOS app. */
class MemberTransferTest {

    private val candidates = listOf(
        MemberCandidate(0, "CALLING"),
        MemberCandidate(1, "REPEATER"),
        MemberCandidate(2, "SIMPLEX"),
        MemberCandidate(5, "PACKET"),
    )

    @Test
    fun members_are_filtered_out_of_the_available_list() {
        // This is what stops a member being added twice, which the radio would
        // hold as a duplicate entry.
        val available = MemberTransfer.available(candidates, listOf(1))
        assertEquals(listOf(0, 2, 5), available.map { it.index })
    }

    @Test
    fun member_items_keep_membership_order_not_candidate_order() {
        val items = MemberTransfer.memberItems(candidates, listOf(5, 0))
        assertEquals(listOf(5, 0), items.map { it.index })
        assertEquals(listOf("PACKET", "CALLING"), items.map { it.label })
    }

    @Test
    fun a_member_whose_record_was_deleted_still_gets_a_removable_row() {
        // Otherwise it is stranded in the zone invisibly, with no way to get
        // rid of it.
        val items = MemberTransfer.memberItems(candidates, listOf(99))
        assertEquals(1, items.size)
        assertTrue(items[0].label.contains("missing"))
    }

    @Test
    fun adding_appends_in_candidate_order_not_selection_order() {
        // A set has no order, so without this a multi-select add would land in
        // an arbitrary sequence, and a zone's order is the order the radio
        // steps through its channels.
        val next = MemberTransfer.adding(setOf(5, 0), candidates, members = listOf(1))
        assertEquals(listOf(1, 0, 5), next)
    }

    @Test
    fun adding_ignores_indices_that_are_already_members() {
        val next = MemberTransfer.adding(setOf(1), candidates, members = listOf(1))
        assertEquals(listOf(1), next)
    }

    @Test
    fun removing_preserves_the_order_of_the_rest() {
        val next = MemberTransfer.removing(setOf(1), members = listOf(0, 1, 2, 5))
        assertEquals(listOf(0, 2, 5), next)
    }

    @Test
    fun reordering_moves_one_entry_and_leaves_the_rest_in_order() {
        assertEquals(listOf(1, 0, 2), MemberTransfer.reordering(listOf(0, 1, 2), from = 0, to = 1))
        assertEquals(listOf(2, 0, 1), MemberTransfer.reordering(listOf(0, 1, 2), from = 2, to = 0))
    }

    @Test
    fun reordering_out_of_bounds_is_a_no_op_rather_than_a_crash() {
        val members = listOf(0, 1, 2)
        assertEquals(members, MemberTransfer.reordering(members, from = 0, to = 9))
        assertEquals(members, MemberTransfer.reordering(members, from = -1, to = 0))
        assertEquals(members, MemberTransfer.reordering(members, from = 1, to = 1))
    }
}
