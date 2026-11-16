# Papyrus tools

Libraries and tools to work with the Papyrus source files and compiled PEX scripts.
This repo includes:
- `pex`: A crate to read `.pex` files (compiled papyrus scripts)
- `pex-as`: An assembler for pex files (unrelated to the Papyrus assembler shipped with the CK, uses a different syntax).
- `pscc`: A papyrus compiler.

## Building from source

Install the latest stable rust toolchain version.

```
git clone https://github.com/openfo4/psc-tools
cd psc-tools
cargo b -r
```

The output binaries can be found in `target/release`.

## AI/LLM Policy

We do not accept AI generated content.
Please do not create issues or PRs containing AI generated comments, code or documentation.
