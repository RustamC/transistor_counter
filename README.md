Simple program that counts the total number of transistors in a gate-level Verilog netlist using CDL file.

# Requirements: 
* [Rust compiler](https://www.rust-lang.org/learn/get-started)

# Build instructions
```
cargo build --release
```

# Usage
```bash
cargo run -r -- lib.cdl netlist.v
```

# Acknowledgement
Verilog parser wrapper is based on https://github.com/sgherbst/svinst
