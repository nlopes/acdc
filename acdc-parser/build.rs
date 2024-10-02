fn main() {
    println!("cargo::rerun-if-changed=fixtures/tests");
    println!("cargo::rerun-if-env-changed=BASE_TEST_DIR");
}
