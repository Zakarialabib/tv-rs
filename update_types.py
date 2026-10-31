import sys

def main():
    with open('crates/tv-rs/src/types.rs', 'r') as f:
        content = f.read()

    # Add GoogleCast to enum
    insert_idx = content.find('Philips,')
    if insert_idx != -1:
        insert_idx += len('Philips,')
        content = content[:insert_idx] + '\n    /// Google Cast or Chromecast.\n    GoogleCast,' + content[insert_idx:]

    # Add as_str
    insert_idx = content.find('Self::Philips => "philips",')
    if insert_idx != -1:
        insert_idx += len('Self::Philips => "philips",')
        content = content[:insert_idx] + '\n            Self::GoogleCast => "googlecast",' + content[insert_idx:]

    # Add control_port
    insert_idx = content.find('Self::Philips => Some(SICP_TCP_PORT),')
    if insert_idx != -1:
        insert_idx += len('Self::Philips => Some(SICP_TCP_PORT),')
        content = content[:insert_idx] + '\n            Self::GoogleCast => Some(8009),' + content[insert_idx:]

    with open('crates/tv-rs/src/types.rs', 'w') as f:
        f.write(content)

if __name__ == '__main__':
    main()
