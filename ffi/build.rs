fn main() {
    uniffi::generate_scaffolding("src/openclipboard.udl").expect("uniffi scaffolding");
}
