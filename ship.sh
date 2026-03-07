export RUSTFLAGS="-Zlocation-detail=none -Zfmt-debug=none"
build="cargo +nightly build -Z build-std=std,panic_abort --profile=pak"
$build --target x86_64-unknown-linux-gnu
$build --target x86_64-pc-windows-gnu
zip --junk-path uusiin_ulottuvuuksiin.zip target/x86_64-unknown-linux-gnu/pak/uusiin_ulottuvuuksiin target/x86_64-pc-windows-gnu/pak/uusiin_ulottuvuuksiin.exe
