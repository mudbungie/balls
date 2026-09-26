//! The README quotes the shipped hook seed verbatim; the seed file is the
//! source of truth, so the quote must match it byte for byte (bl-bedb).

#[test]
fn readme_quotes_the_seed_hooks_table_verbatim() {
    let seed = include_str!("../default-config/plugins.toml");
    let readme = include_str!("../README.md");
    let table = &seed[seed.find("\n[hooks]\n").expect("seed has a [hooks] table") + 1..];
    let start = readme.find("```toml\n[hooks]\n").expect("README quotes the seed") + "```toml\n".len();
    let end = start + readme[start..].find("```").expect("closing fence");
    assert_eq!(&readme[start..end], table.trim_end().to_owned() + "\n");
}
