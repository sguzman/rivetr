fn main() {
    if let Err(error) = rivet_app::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
