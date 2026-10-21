// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.24;

contract TestToken {
    mapping(address account => uint256 balance) public balanceOf;
    mapping(address owner => mapping(address spender => uint256 amount)) public allowance;

    function mint(address account, uint256 amount) external {
        balanceOf[account] += amount;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transfer(address recipient, uint256 amount) external returns (bool) {
        _move(msg.sender, recipient, amount);
        return true;
    }

    function transferFrom(address sender, address recipient, uint256 amount) external returns (bool) {
        uint256 approved = allowance[sender][msg.sender];
        require(approved >= amount, "allowance");
        allowance[sender][msg.sender] = approved - amount;
        _move(sender, recipient, amount);
        return true;
    }

    function _move(address sender, address recipient, uint256 amount) internal virtual {
        uint256 balance = balanceOf[sender];
        require(balance >= amount, "balance");
        balanceOf[sender] = balance - amount;
        balanceOf[recipient] += amount;
    }
}

contract FeeOnTransferToken is TestToken {
    function _move(address sender, address recipient, uint256 amount) internal override {
        uint256 fee = amount / 10;
        uint256 received = amount - fee;
        uint256 balance = balanceOf[sender];
        require(balance >= amount, "balance");
        balanceOf[sender] = balance - amount;
        balanceOf[recipient] += received;
    }
}
