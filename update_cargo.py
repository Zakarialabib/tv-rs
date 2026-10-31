import sys

def main():
    with open('crates/tv-rs/Cargo.toml', 'r') as f:
        content = f.read()

    # Add tv-rs-cast to dependencies
    deps_index = content.find('[dependencies]')
    if deps_index != -1:
        insert_idx = content.find('tv-rs-roku.workspace = true', deps_index)
        if insert_idx != -1:
            insert_idx += len('tv-rs-roku.workspace = true')
            content = content[:insert_idx] + '\ntv-rs-cast = { path = "../cast", version = "0.5.0", optional = true }' + content[insert_idx:]

    # Add cast feature
    features_index = content.find('[features]')
    if features_index != -1:
        insert_idx = content.find('hil = []', features_index)
        if insert_idx != -1:
            content = content[:insert_idx] + 'cast = ["tv-rs-cast"]\n' + content[insert_idx:]

    with open('crates/tv-rs/Cargo.toml', 'w') as f:
        f.write(content)

if __name__ == '__main__':
    main()
