package me.faulk.codeplugme.state

import me.faulk.codeplugme.model.Capabilities
import me.faulk.codeplugme.model.DumpAprs
import me.faulk.codeplugme.model.DumpChannel
import me.faulk.codeplugme.model.DumpContact
import me.faulk.codeplugme.model.DumpGroupList
import me.faulk.codeplugme.model.DumpQytBand
import me.faulk.codeplugme.model.DumpQytChannel
import me.faulk.codeplugme.model.DumpQytSettings
import me.faulk.codeplugme.model.DumpRadioId
import me.faulk.codeplugme.model.DumpScanList
import me.faulk.codeplugme.model.DumpZone

/**
 * Everything the editor screens render, as one immutable value.
 *
 * The records here are always a fresh parse of the work file on disk. The app
 * never models a half-applied edit: an edit goes to the core, the core rewrites
 * the work file, and the whole state is rebuilt from what came back. That is
 * what makes undo a byte snapshot and what makes an unmodelled field survive an
 * edit it was never part of.
 */
data class CodeplugState(
    /** The library entry being edited, or null when nothing is open. */
    val entryId: String? = null,
    val entryName: String = "",
    val modelId: String? = null,
    val displayName: String = "",
    val capabilities: Capabilities? = null,

    // AnyTone families.
    val channels: List<DumpChannel> = emptyList(),
    val zones: List<DumpZone> = emptyList(),
    val scanLists: List<DumpScanList> = emptyList(),
    val contacts: List<DumpContact> = emptyList(),
    val groupLists: List<DumpGroupList> = emptyList(),
    val radioIds: List<DumpRadioId> = emptyList(),
    val aprs: DumpAprs? = null,

    // QYT families.
    val qytChannels: List<DumpQytChannel> = emptyList(),
    val qytSettings: DumpQytSettings? = null,
    val qytBands: List<DumpQytBand> = emptyList(),
    /** The LCD static message at `0x0FE0`, empty with no QYT codeplug open. */
    val qytStaticMessage: String = "",

    /** True when the work file holds edits the stored codeplug does not. */
    val isDirty: Boolean = false,
    /** Per-section slots carrying a staged change, for the row indicators. */
    val unsavedSlots: Map<CodeplugSection, Set<Int>> = emptyMap(),
    /** Sections carrying any staged change, for the navigation indicators. */
    val unsavedSections: Set<CodeplugSection> = emptySet(),

    val canUndo: Boolean = false,
    val canRedo: Boolean = false,

    val status: String? = null,
    val error: String? = null,
) {
    /** True when a codeplug is open for editing. */
    val isOpen: Boolean get() = entryId != null

    /**
     * The sections this radio has, in the order the core lists them.
     *
     * Unknown keys are dropped rather than failing, so a core that grows a
     * section this build has no screen for degrades to hiding it.
     */
    val sections: List<CodeplugSection>
        get() = capabilities?.sections.orEmpty().mapNotNull { CodeplugSection.fromKey(it) }

    /** True when `slot` in `section` holds a staged change. */
    fun isUnsaved(section: CodeplugSection, slot: Int): Boolean =
        unsavedSlots[section]?.contains(slot) == true

    /** True when `section` holds any staged change. */
    fun hasUnsavedChanges(section: CodeplugSection): Boolean = section in unsavedSections
}
