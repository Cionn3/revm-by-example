// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.7.0 <0.9.0;

import {IERC20} from './lib/interfaces/IERC20.sol';
import {SafeERC20} from './lib/interfaces/SafeERC20/SafeERC20.sol';
import {Swapper} from './lib/Swapper.sol';
import {IUniswapV3SwapCallback, IUniswapV3Factory} from './lib/interfaces/Uniswap.sol';


contract SwapRouter {

    address internal constant V3_FACTORY = 0x1F98431c8aD98523631AE4a59f267346ea31F984;

    constructor() {}

    using SafeERC20 for IERC20;


    function do_swap(Swapper.Params calldata params) external returns (uint256 real_amount) {
        real_amount = Swapper.swap(params, msg.sender);
        require(real_amount >= params.minimum_received, "Real Amount < Minimum Received");
        return real_amount;
    }


     // ! In Production you need to lock this function so only the owner can call it
    function recover_erc20(address token, uint256 amount) external {
        IERC20(token).safeTransfer(msg.sender, amount);
    }


        function uniswapV3SwapCallback(
        int256 amount0Delta,
        int256 amount1Delta,
        bytes calldata data
    ) external {
        (bool zeroForOne, address input_token, address output_token, address caller, uint24 fee) = abi.decode(
            data,
            (bool, address, address, address, uint24)
        );

        // verify that msg.sender is a valid pool
        address pool = IUniswapV3Factory(V3_FACTORY).getPool(input_token, output_token, fee);
        require(msg.sender == pool, "Not the pool");

        if (zeroForOne) {
        IERC20(input_token).safeTransferFrom(caller, pool, uint(amount0Delta));
        } else {
        IERC20(input_token).safeTransferFrom(caller, pool, uint(amount1Delta));
            }
    }

}