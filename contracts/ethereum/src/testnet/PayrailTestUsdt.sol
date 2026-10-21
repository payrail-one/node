// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.24;

/// @notice Fixed-supply test token that mirrors the six-decimal, no-return
/// transfer ABI relevant to Ethereum mainnet USDt integration.
/// @dev Testnet-only. This token is not issued, backed or endorsed by Tether.
contract PayrailTestUsdt {
    string public constant name = "Payrail Test USDt";
    string public constant symbol = "tUSDT";
    uint8 public constant decimals = 6;

    uint256 public immutable totalSupply;
    mapping(address account => uint256 balance) public balanceOf;
    mapping(address owner => mapping(address spender => uint256 amount)) public allowance;

    event Transfer(address indexed sender, address indexed recipient, uint256 amount);
    event Approval(address indexed owner, address indexed spender, uint256 amount);

    constructor(address initialHolder, uint256 initialSupply) {
        require(initialHolder != address(0), "holder");
        require(initialSupply != 0, "supply");
        totalSupply = initialSupply;
        balanceOf[initialHolder] = initialSupply;
        emit Transfer(address(0), initialHolder, initialSupply);
    }

    function approve(address spender, uint256 amount) external {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
    }

    function transfer(address recipient, uint256 amount) external {
        _move(msg.sender, recipient, amount);
    }

    function transferFrom(address sender, address recipient, uint256 amount) external {
        uint256 approved = allowance[sender][msg.sender];
        require(approved >= amount, "allowance");
        allowance[sender][msg.sender] = approved - amount;
        _move(sender, recipient, amount);
    }

    function _move(address sender, address recipient, uint256 amount) private {
        require(recipient != address(0), "recipient");
        uint256 balance = balanceOf[sender];
        require(balance >= amount, "balance");
        balanceOf[sender] = balance - amount;
        balanceOf[recipient] += amount;
        emit Transfer(sender, recipient, amount);
    }
}
