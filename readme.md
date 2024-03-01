# Revm by Example

## Practical examples of the Rust Ethereum Virtual Machine ([REVM](https://github.com/bluealloy/revm))

## Getting Started

**Clone the Repository:**

`git clone https://github.com/Cionn3/revm-by-example.git`

## Usage

**To run an example, cargo run by the module name:**
`cargo run --bin simulate-call`

### Available Examples

- **simulate_call.rs**: Simulates interactions with the WETH contract.

- **simulate_swap.rs**: Demonstrates a token swap on a Uniswap pool, interacting with a custom Solidity contract, and withdrawing ERC20 tokens to the caller's account.