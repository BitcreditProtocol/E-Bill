# WASM Releasing

To release a new `WASM` version, you can just use the `WASM Release` action, defined in `.github/workflows/wasm_release.yml`.

The only input it takes is an optional changelog and it will:

* Check if the current version (as compared with `crates/bcr-ebill-wasm/Cargo.toml`) already has an existing git tag - if yes, it fails
* Build the WASM artifacts for `@bitcredit/bcr-ebill-wasm`
* Create and push a tag for the current version
* Create a release for the current version
* Upload the WASM artifacts to the release
* Publish the WASM artifacts to the [npm package](https://www.npmjs.com/package/@bitcredit/bcr-ebill-wasm)

