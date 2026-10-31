import sys
import re

def main():
    with open('crates/tv-rs/src/lib.rs', 'r') as f:
        content = f.read()

    # Add cast to TvVendor enum if not present
    if 'GoogleCast' not in content:
        # Unfortunately TvVendor is in tv-rs-core! Let's check tv-rs-core
        pass

if __name__ == '__main__':
    main()
