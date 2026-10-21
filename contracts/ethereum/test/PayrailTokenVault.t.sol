// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.24;

import { PayrailTokenVault } from "../src/PayrailTokenVault.sol";
import { PayrailTestUsdt } from "../src/testnet/PayrailTestUsdt.sol";
import { FeeOnTransferToken } from "./TestTokens.sol";

contract TokenVaultActor {
    receive() external payable {}

    function approveAndDeposit(
        PayrailTokenVault vault,
        PayrailTestUsdt token,
        bytes32 recipient,
        uint256 amount
    ) external {
        token.approve(address(vault), amount);
        vault.deposit(recipient, amount);
    }

    function release(
        PayrailTokenVault vault,
        bytes32 withdrawalId,
        address recipient,
        uint256 amount
    ) external {
        vault.release(withdrawalId, recipient, amount);
    }

    function callRaw(address target, bytes calldata data) external returns (bool success) {
        (success, ) = target.call(data);
    }
}

contract PayrailTokenVaultTest {
    bytes32 private constant PAYRAIL_RECIPIENT = bytes32(uint256(41));

    PayrailTestUsdt private token;
    PayrailTokenVault private vault;
    TokenVaultActor private depositor;
    TokenVaultActor private releaseAuthority;
    TokenVaultActor private recipient;

    function setUp() public {
        depositor = new TokenVaultActor();
        releaseAuthority = new TokenVaultActor();
        recipient = new TokenVaultActor();
        token = new PayrailTestUsdt(address(depositor), 10_000_000_000);
        vault = new PayrailTokenVault(address(token), address(this), address(releaseAuthority));
    }

    function testNoReturnUsdtDepositReleaseReplayAndPause() public {
        depositor.approveAndDeposit(vault, token, PAYRAIL_RECIPIENT, 5_000_000);
        require(token.balanceOf(address(vault)) == 5_000_000, "vault deposit");
        require(vault.depositNonce() == 1, "deposit nonce");

        bytes32 withdrawal = bytes32(uint256(42));
        releaseAuthority.release(vault, withdrawal, address(recipient), 2_000_000);
        require(token.balanceOf(address(vault)) == 3_000_000, "vault release");
        require(token.balanceOf(address(recipient)) == 2_000_000, "recipient release");
        require(vault.released(withdrawal), "release replay mark");

        bool replay = releaseAuthority.callRaw(
            address(vault),
            abi.encodeCall(PayrailTokenVault.release, (withdrawal, address(recipient), 1))
        );
        require(!replay, "replay accepted");

        vault.setPaused(true);
        bool paused = depositor.callRaw(
            address(depositor),
            abi.encodeCall(
                TokenVaultActor.approveAndDeposit,
                (vault, token, PAYRAIL_RECIPIENT, 1)
            )
        );
        require(!paused, "paused deposit accepted");
        vault.setPaused(false);
    }

    function testImmutableTokenAndRoleSeparation() public {
        require(vault.token() == address(token), "wrong immutable token");
        (bool sameRelease, ) = address(vault).call(
            abi.encodeCall(PayrailTokenVault.setReleaseAuthority, (address(this)))
        );
        require(!sameRelease, "owner accepted as release authority");
        (bool releaseAsOwner, ) = address(vault).call(
            abi.encodeCall(PayrailTokenVault.transferOwnership, (address(releaseAuthority)))
        );
        require(!releaseAsOwner, "release authority accepted as owner");

        vault.transferOwnership(address(recipient));
        bool accepted = recipient.callRaw(
            address(vault),
            abi.encodeCall(PayrailTokenVault.acceptOwnership, ())
        );
        require(accepted, "pending owner rejected");
        require(vault.owner() == address(recipient), "owner not transferred");
    }

    function testFeeOnTransferTokenRollsBack() public {
        FeeOnTransferToken feeToken = new FeeOnTransferToken();
        PayrailTokenVault feeVault = new PayrailTokenVault(
            address(feeToken),
            address(this),
            address(releaseAuthority)
        );
        feeToken.mint(address(this), 1_000);
        feeToken.approve(address(feeVault), 100);
        (bool success, ) = address(feeVault).call(
            abi.encodeCall(PayrailTokenVault.deposit, (PAYRAIL_RECIPIENT, 100))
        );
        require(!success, "fee token accepted");
        require(feeToken.balanceOf(address(feeVault)) == 0, "vault retained failed deposit");
        require(feeToken.balanceOf(address(this)) == 1_000, "sender lost failed deposit");
    }

    function testFuzzNoReturnUsdtDepositAndRelease(uint72 rawAmount, uint72 rawRelease) public {
        uint256 amount = (uint256(rawAmount) % 1_000_000_000) + 1;
        uint256 releaseAmount = (uint256(rawRelease) % amount) + 1;
        depositor.approveAndDeposit(vault, token, PAYRAIL_RECIPIENT, amount);
        releaseAuthority.release(
            vault,
            bytes32(uint256(43)),
            address(recipient),
            releaseAmount
        );
        require(token.balanceOf(address(vault)) == amount - releaseAmount, "vault delta");
        require(token.balanceOf(address(recipient)) == releaseAmount, "recipient delta");
    }
}
