import sys
import re

def main():
    # fix handle.rs
    with open('crates/tv-rs/src/handle.rs', 'r') as f:
        content = f.read()

    insert_idx = content.find('TvVendor::Philips => DeviceOs::HostMonitor,')
    if insert_idx != -1:
        insert_idx += len('TvVendor::Philips => DeviceOs::HostMonitor,')
        content = content[:insert_idx] + '\n        TvVendor::GoogleCast => DeviceOs::GoogleCast,' + content[insert_idx:]

    with open('crates/tv-rs/src/handle.rs', 'w') as f:
        f.write(content)

    # check if DeviceOs::GoogleCast exists in tv-rs-core
    pass

if __name__ == '__main__':
    main()
