// SPDX-License-Identifier: MIT
pragma solidity >=0.7.0 <0.9.0;
// Enable ABI encoder v2
pragma abicoder v2;

// Uniswap v2 v3 interfaces
import {IUniswapV2Pair, IUniswapV3Pool} from './interfaces/Uniswap.sol';

// ERC20 interface
import './interfaces/IERC20.sol'; 
import {SafeERC20} from './interfaces/SafeERC20/SafeERC20.sol';

library Swapper {
    using SafeERC20 for IERC20;

    uint160 internal constant MIN_SQRT_RATIO = 4295128749;
    uint160 internal constant MAX_SQRT_RATIO = 1461446703485210103287273052203988822378723970341;

    // Parameters for the swap
    struct Params {
        address input_token;
        address output_token;
        uint256 amount_in;
        address pool;
        uint pool_variant;
        uint256 minimum_received;
    }




function swap(
    Params calldata params, address caller
) internal returns (uint256 real_amount) {

     uint256 initial_balance;
     uint256 amount_out;

    initial_balance = IERC20(params.output_token).balanceOf(caller);

    if (params.pool_variant == 0) {
        amount_out = swap_on_V2(params, caller);
    } else
    
    if (params.pool_variant == 1) {
        amount_out = swap_on_V3(params, caller);
        
    } else {
        revert("Invalid pool variant");
    }

        // if we have any token balance left make sure we substract it from the amount out
       real_amount = (initial_balance > 0) ? (amount_out - initial_balance) : amount_out;
       return real_amount;
}



// from: https://github.com/mouseless-eth/rusty-sando/blob/master/contract/src/LilRouter.sol
// swap input token for output token on uniswap v2 and forks, returns real balance of output token
function swap_on_V2(Params calldata params, address caller) internal returns(uint256) {

   
        // Optimistically send amountIn of inputToken to targetPair
        IERC20(params.input_token).safeTransferFrom(caller, address(this), params.amount_in);

        // Prepare variables for calculating expected amount out
        uint reserveIn;
        uint reserveOut;


        { // Avoid stack too deep error
        (uint reserve0, uint reserve1,) = IUniswapV2Pair(params.pool).getReserves();

        // sort reserves
        if (params.input_token < params.output_token) {
            // Token0 is equal to inputToken
            // Token1 is equal to outputToken
            reserveIn = reserve0;
            reserveOut = reserve1;
        } else {
            // Token0 is equal to outputToken
            // Token1 is equal to inputToken
            reserveIn = reserve1;
            reserveOut = reserve0;
        }
        }



        // Find the actual amountIn sent to pair (accounts for tax if any) and amountOut
       uint actualAmountIn = IERC20(params.input_token).balanceOf(address(params.pool)) - reserveIn;
       uint256 amountOut = _getAmountOut(actualAmountIn, reserveIn, reserveOut);

        // Prepare swap variables and call pair.swap()
        (uint amount0Out, uint amount1Out) = params.input_token < params.output_token ? (uint(0), amountOut) : (amountOut, uint(0));
        IUniswapV2Pair(params.pool).swap(amount0Out, amount1Out, caller, new bytes(0));

     return IERC20(params.output_token).balanceOf(caller);

}

// from: https://github.com/mouseless-eth/rusty-sando/blob/master/contract/src/LilRouter.sol
// swaps input token for output token on uniswap v3 and forks, returns real balance of output token
function swap_on_V3(Params calldata params, address caller) internal returns (uint256) {
    
    bool zeroForOne = params.input_token < params.output_token;
    uint160 sqrtPriceLimitX96 = zeroForOne ? MIN_SQRT_RATIO : MAX_SQRT_RATIO;

    uint24 fee = IUniswapV3Pool(params.pool).fee();

    IUniswapV3Pool(params.pool).swap(
        caller, 
        zeroForOne, 
        int256(params.amount_in),
        sqrtPriceLimitX96,
        abi.encode(zeroForOne, params.input_token, params.output_token, caller, fee) 
    );

    return IERC20(params.output_token).balanceOf(caller);
}

function _getAmountOut(uint amountIn, uint reserveIn, uint reserveOut) internal pure returns (uint amountOut) {
    require(amountIn > 0, 'UniswapV2Library: INSUFFICIENT_INPUT_AMOUNT');
    require(reserveIn > 0 && reserveOut > 0, 'UniswapV2Library: INSUFFICIENT_LIQUIDITY');
    uint amountInWithFee = amountIn * 997;
    uint numerator = amountInWithFee * reserveOut;
    uint denominator = reserveIn * 1000 + amountInWithFee;
    amountOut = numerator / denominator;
}

}