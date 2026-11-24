"""Install the two verified builds behind one wasm-bindgen initialization API."""
from pathlib import Path
import shutil
import sys
from tempfile import TemporaryDirectory


compatible, accelerated = map(Path, sys.argv[1:])
script_name = "saturn_rust.js"
wasm_name = "saturn_rust_bg.wasm"
fallback_name = "saturn_rust_compatible_bg.wasm"
glue_path = compatible / script_name
glue = glue_path.read_text()
if glue != (accelerated / script_name).read_text():
    raise SystemExit("The Wasm variants generated different wasm-bindgen bindings")

default_url = "        module_or_path = new URL('saturn_rust_bg.wasm', import.meta.url);"
if glue.count(default_url) != 1:
    raise SystemExit("Unexpected wasm-bindgen default module path")
selector = """        // (module (func) (func (return_call 0)))
        const tailCallProbe = new Uint8Array([
            0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0,
            3, 3, 2, 0, 0, 10, 9, 2, 2, 0, 11, 4, 0, 18, 0, 11,
        ]);
        const filename = WebAssembly.validate(tailCallProbe)
            ? 'saturn_rust_bg.wasm'
            : 'saturn_rust_compatible_bg.wasm';
        module_or_path = new URL(filename, import.meta.url);"""
with TemporaryDirectory(dir=compatible, prefix=".saturn-variants-") as directory:
    stage = Path(directory)
    shutil.copy(compatible / wasm_name, stage / fallback_name)
    shutil.copy(accelerated / wasm_name, stage / wasm_name)
    (stage / script_name).write_text(glue.replace(default_url, selector))
    (stage / fallback_name).replace(compatible / fallback_name)
    (stage / script_name).replace(glue_path)
    (stage / wasm_name).replace(compatible / wasm_name)
