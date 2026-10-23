# EDID fixtures

Real EDIDs captured from hardware, as `edid-decode (hex)` dumps (16 bytes per line,
every block). The expected values in `src/edid.rs` tests come from the
`edid-decode` output published next to each dump.

| File                          | Display                         | Source                                                                                                    |
| ----------------------------- | ------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `lg-tv-gsm0001.hex`           | LG TV (GSM0001), 2015, 160×90cm | [linuxhw/EDID `Digital/Goldstar/GSM0001/009307C6E167`](https://github.com/linuxhw/EDID/tree/master/Digital/Goldstar/GSM0001) |
| `samsung-c49rg9x-sam0f99.hex` | Samsung C49RG9x (SAM0F99), 2020 | [linuxhw/EDID `Digital/Samsung/SAM0F99/01312B4953B0`](https://github.com/linuxhw/EDID/tree/master/Digital/Samsung/SAM0F99) |

Both come from the [linuxhw/EDID](https://github.com/linuxhw/EDID) database by the
Linux Hardware Project, licensed under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). The dumps are unmodified;
the database anonymises serial numbers before publishing them.
