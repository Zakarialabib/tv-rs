import sys

def fix_file(filepath):
    with open(filepath, 'r') as f:
        lines = f.readlines()

    out = []
    for line in lines:
        out.append(line)
        if 'TvVendor::Unknown => DeviceOs::Unknown,' in line:
            out.append('        TvVendor::GoogleCast => DeviceOs::GoogleCast,\n')
        elif 'TvVendor::Unknown => Err(TvError::UnsupportedDevice(tv.vendor.clone())),' in line:
            out.append('            TvVendor::GoogleCast => Err(TvError::UnsupportedDevice(tv.vendor.clone())),\n')

    with open(filepath, 'w') as f:
        f.writelines(out)

fix_file('crates/tv-rs/src/client.rs')
fix_file('crates/tv-rs/src/handle.rs')
