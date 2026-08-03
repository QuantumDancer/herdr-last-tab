// Real subcommand dispatch arrives in T016, once the Herdr trait (T015) exists to make
// it testable. Until then, a placeholder that always reports usage keeps `cargo build`
// and every CI gate in ci.yml green, per T007.
fn main() {
    eprintln!("usage: herdr-last-tab <toggle|tab-focused|tab-closed|workspace-closed>");
    std::process::exit(2);
}
