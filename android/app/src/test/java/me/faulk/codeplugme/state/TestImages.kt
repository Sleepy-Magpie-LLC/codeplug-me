package me.faulk.codeplugme.state

/**
 * Codeplug images built in memory, so the test suite needs no committed binary
 * fixture and nothing from a real radio.
 *
 * The empty shapes are what `Model::detect` in the core actually accepts: an
 * AnyTone image is identified by its length alone, and a QYT image by its
 * length plus the ASCII model tag.
 *
 * The populated shapes are built by asking the core to add records, rather than
 * by hand-encoding bytes here. Hand-encoding would mean this file quietly
 * carrying a second, unverified copy of the memory map, and it would drift.
 */
object TestImages {

    /** Byte length of an AnyTone AT-D878UVII Plus image. */
    const val ANYTONE_SIZE = 1_789_328

    /** Byte length of a QYT KT-5000 image. */
    const val QYT_SIZE = 0x3200

    /**
     * An empty but valid AnyTone codeplug.
     *
     * All-zero parses cleanly: the channel and zone bitmaps read as "nothing
     * active", which is exactly an empty radio.
     */
    fun anytone(): ByteArray = ByteArray(ANYTONE_SIZE)

    /**
     * An empty but valid QYT codeplug.
     *
     * 0xFF is what erased flash reads back as and is the empty-slot marker, and
     * the model tag at 0x0FE0 is what lets the core tell a KT-5000 image from
     * any other 12,800-byte file.
     */
    fun qytEmpty(): ByteArray {
        val img = ByteArray(QYT_SIZE) { 0xFF.toByte() }
        val tag = "Model:KT-5000   ".toByteArray(Charsets.US_ASCII)
        tag.copyInto(img, 0x0FE0)
        return img
    }

    /**
     * A QYT codeplug with three active channels.
     *
     * Most tests need a record that actually exists, because the core rejects
     * an edit aimed at an empty slot, which is correct behaviour and not
     * something to work around.
     */
    fun qyt(): ByteArray = uniffi.codeplugme_core.applyEdits(
        qytEmpty(),
        """
        {"qyt":{"add_channels":[
          {"name":"FRS-01","rx_frequency_hz":462562500,"tx_frequency_hz":462562500},
          {"name":"FRS-02","rx_frequency_hz":462587500,"tx_frequency_hz":462587500},
          {"name":"RX-ONLY","rx_frequency_hz":146520000}
        ]}}
        """.trimIndent(),
    )

    /**
     * An AnyTone codeplug with one channel, one zone, and one contact.
     *
     * Enough for an edit to have something to aim at without making the image
     * slow to parse: the AnyTone always reports its full 10,000 contact slots.
     */
    fun anytonePopulated(): ByteArray = uniffi.codeplugme_core.applyEdits(
        anytone(),
        """
        {"anytone":{
          "add_channels":[{"name":"CALLING","rx_frequency_hz":146520000,"tx_frequency_hz":146520000}],
          "add_zones":[{"name":"Zone A","members":[0]}],
          "add_contacts":[{"name":"Local","number":3113043}]
        }}
        """.trimIndent(),
    )
}
