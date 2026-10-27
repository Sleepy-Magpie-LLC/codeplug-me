package me.faulk.codeplugme.model

/**
 * Real `codeplugme_dump_json` output captured from the Rust core, used to prove
 * the Kotlin model layer decodes what the core actually emits rather than
 * what a hand-written fixture assumes it emits.
 */
object TestFixtures {
    val ANYTONE_ENVELOPE = """
        {
          "anytone": {
            "aprs": {
              "auto_tx_interval": 0,
              "destination_call": "",
              "destination_ssid": 0,
              "fm_power": 0,
              "fm_tx_frequency_hz": 0,
              "manual_tx_interval": 0,
              "source_call": "",
              "source_ssid": 0,
              "symbol": 0,
              "symbol_table": 0
            },
            "channels": [
              {
                "admit": "Always",
                "bandwidth": "Narrow",
                "call_confirm": false,
                "color_code": 0,
                "contact_index": 0,
                "dtmf_id_index": 0,
                "five_tone_id_index": 0,
                "group_list_index": 0,
                "index": 0,
                "mode": "Analog",
                "name": "CALLING",
                "optional_signaling": "Off",
                "power": "Low",
                "radio_id_index": 0,
                "repeater_mode": "Simplex",
                "rx_aprs": false,
                "rx_ctcss": 0,
                "rx_dcs": 0,
                "rx_frequency_hz": 146520000,
                "rx_only": false,
                "rx_signaling_mode": "None",
                "scan_list_index": 0,
                "simplex_tdma": false,
                "squelch_mode": "Carrier",
                "talk_around": false,
                "time_slot": 1,
                "two_tone_decode_index": 0,
                "two_tone_id_index": 0,
                "tx_ctcss": 0,
                "tx_dcs": 0,
                "tx_frequency_hz": 146520000,
                "tx_offset_hz": 0,
                "tx_signaling_mode": "None",
                "work_alone": false
              }
            ],
            "contacts": [
              { "call_type": "Private", "index": 0, "name": "", "number": 0 }
            ],
            "group_lists": [
              { "index": 0, "members": [], "name": "RX Grp" }
            ],
            "radio_ids": [
              { "index": 0, "name": "Me", "number": 3113043 }
            ],
            "scan_lists": [
              {
                "dropout_delay": 0,
                "dwell_time": 0,
                "index": 0,
                "look_back_a": 0,
                "look_back_b": 0,
                "members": [],
                "name": "Scan 1",
                "priority_channel_1": 65535,
                "priority_channel_2": 65535,
                "priority_channel_select": 0,
                "revert_channel": 0
              }
            ],
            "zones": [
              { "a_channel": 0, "b_channel": 0, "channels": [], "index": 0, "name": "Zone A" }
            ]
          },
          "capabilities": {
            "bandwidth_options": [
              { "label": "Narrow", "tag": "Narrow" },
              { "label": "Wide", "tag": "Wide" }
            ],
            "channel_capacity": 4000,
            "channel_name_length": 16,
            "power_options": [
              { "label": "Low", "tag": "Low" },
              { "label": "Mid", "tag": "Mid" },
              { "label": "High", "tag": "High" },
              { "label": "Turbo", "tag": "Turbo" }
            ],
            "sections": [
              "channels", "zones", "scan_lists", "contacts", "group_lists", "radio_ids", "aprs"
            ],
            "supports_dmr": true,
            "tone_encoding": "table_index"
          },
          "display_name": "AnyTone AT-D878UVII Plus",
          "model": "anytone-d878uv2"
        }
    """.trimIndent()

    val QYT_ENVELOPE = """
        {
          "capabilities": {
            "bandwidth_options": [
              { "label": "Wide", "tag": "Wide" },
              { "label": "Narrow", "tag": "Narrow" }
            ],
            "channel_capacity": 200,
            "channel_name_length": 7,
            "power_options": [
              { "label": "High (25 W)", "tag": "High" },
              { "label": "Low (5 W)", "tag": "Low" }
            ],
            "sections": ["qyt_channels", "qyt_settings", "qyt_bands"],
            "supports_dmr": false,
            "tone_encoding": "hz_tenths"
          },
          "display_name": "QYT KT-5000",
          "model": "qyt-kt5000",
          "qyt": {
            "bands": [
              {
                "bandwidth": "Wide",
                "frequency_hz": 462562500,
                "index": 0,
                "offset_hz": 10000,
                "optional_signaling": "OFF",
                "power": "High",
                "programmed": true,
                "rx_tone": { "kind": "none" },
                "scode": 0,
                "scramble": false,
                "shift": "Off",
                "speaker_mute": "Tone/DTCS",
                "step_hz": 2500,
                "tx_tone": { "kind": "none" }
              },
              {
                "bandwidth": "Wide",
                "frequency_hz": 462615000,
                "index": 1,
                "offset_hz": 21000,
                "optional_signaling": "OFF",
                "power": "High",
                "programmed": true,
                "rx_tone": { "kind": "none" },
                "scode": 0,
                "scramble": false,
                "shift": "Off",
                "speaker_mute": "Tone/DTCS",
                "step_hz": 2500,
                "tx_tone": { "kind": "none" }
              }
            ],
            "channels": [
              {
                "bandwidth": "Wide",
                "busy_lockout": false,
                "flags_raw": [0, 0, 0, 36],
                "index": 0,
                "name": "FRS-01",
                "optional_signaling": "OFF",
                "power": "High",
                "ptt_id": "OFF",
                "rx_frequency_hz": 462562500,
                "rx_tone": { "kind": "none" },
                "scan_add": true,
                "scode": 0,
                "scramble": false,
                "speaker_mute": "Tone/DTCS",
                "tx_frequency_hz": 462562500,
                "tx_tone": { "kind": "none" }
              },
              {
                "bandwidth": "Wide",
                "busy_lockout": false,
                "flags_raw": [0, 0, 0, 4],
                "index": 1,
                "name": "FRS-02",
                "optional_signaling": "OFF",
                "power": "High",
                "ptt_id": "OFF",
                "rx_frequency_hz": 462587500,
                "rx_tone": { "kind": "none" },
                "scan_add": true,
                "scode": 0,
                "scramble": false,
                "speaker_mute": "Tone/DTCS",
                "tx_frequency_hz": 462587500,
                "tx_tone": { "kind": "none" }
              }
            ],
            "static_message": "Model:KT-5000",
            "settings": {
              "backlight_timer": 0,
              "beep": false,
              "remember_power": true,
              "scan_rev": "CO",
              "squelch": 6,
              "tmr": 1,
              "tot": 3,
              "tot_seconds": 60,
              "volume": 0
            }
          }
        }
    """.trimIndent()

    val TONE_VARIANTS = """
        [
          { "kind": "none" },
          { "hz_tenths": 885, "kind": "ctcss" },
          { "code": 23, "inverted": false, "kind": "dcs" }
        ]
    """.trimIndent()
}
