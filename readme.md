# Revm by Example

## Practical examples of the Rust Ethereum Virtual Machine ([REVM](https://github.com/bluealloy/revm))

**For usage with [alloy](https://github.com/alloy-rs/alloy) see branch [feat-alloy](https://github.com/Cionn3/revm-by-example/tree/feat-alloy)**

## Getting Started

**Clone the Repository:**

`git clone https://github.com/Cionn3/revm-by-example.git`

## Usage

**To run an example, cargo run by the module name:**
`cargo run --bin simulate-call`

### Available Examples

- **simulate_call.rs**: Simulates interactions with the WETH contract.

- **simulate_swap.rs**: Demonstrates a token swap on a Uniswap pool, interacting with a custom Solidity contract, and withdrawing ERC20 tokens to the caller's account.

- **simple_trace.rs**: An example of how you may trace pending transactions.
