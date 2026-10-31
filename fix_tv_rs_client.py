import sys

def main():
    with open('crates/tv-rs/src/client.rs', 'r') as f:
        content = f.read()

    # We need to map GoogleCast to unsupported or to actual client if feature is enabled.
    # To keep minimal diff, we'll map to TvError::UnsupportedDevice for now as the facade may need trait impls for cast.
    insert_idx = content.find('TvVendor::Philips => Err(TvError::UnsupportedDevice(tv.vendor.clone())),')
    if insert_idx != -1:
        insert_idx += len('TvVendor::Philips => Err(TvError::UnsupportedDevice(tv.vendor.clone())),')
        content = content[:insert_idx] + '\n            TvVendor::GoogleCast => Err(TvError::UnsupportedDevice(tv.vendor.clone())), // Handled via trait if needed' + content[insert_idx:]

    # For the second one
    insert_idx = content.find('TvVendor::Philips => Err(TvError::UnsupportedDevice(tv.vendor.clone())),', insert_idx + 10)
    if insert_idx != -1:
        insert_idx += len('TvVendor::Philips => Err(TvError::UnsupportedDevice(tv.vendor.clone())),')
        content = content[:insert_idx] + '\n            TvVendor::GoogleCast => Err(TvError::UnsupportedDevice(tv.vendor.clone())), // Handled via trait if needed' + content[insert_idx:]

    with open('crates/tv-rs/src/client.rs', 'w') as f:
        f.write(content)

if __name__ == '__main__':
    main()
