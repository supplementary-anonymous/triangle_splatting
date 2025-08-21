#! /bin/sh

wasm-pack build --release --no-opt --target web \
 && python3 -m http.server 8000
