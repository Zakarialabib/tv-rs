import sys

def main():
    with open('crates/core/src/descriptor.rs', 'r') as f:
        content = f.read()

    insert_idx = content.find('PhilipsSicp,')
    if insert_idx != -1:
        insert_idx += len('PhilipsSicp,')
        content = content[:insert_idx] + '\n    /// Google Cast or Chromecast.\n    GoogleCast,' + content[insert_idx:]

    insert_idx = content.find('Self::PhilipsSicp => "philips-sicp",')
    if insert_idx != -1:
        insert_idx += len('Self::PhilipsSicp => "philips-sicp",')
        content = content[:insert_idx] + '\n            Self::GoogleCast => "googlecast",' + content[insert_idx:]

    with open('crates/core/src/descriptor.rs', 'w') as f:
        f.write(content)

if __name__ == '__main__':
    main()
