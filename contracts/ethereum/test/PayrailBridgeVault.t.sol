// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.24;

import { PayrailBridgeVault, IERC20 } from "../src/PayrailBridgeVault.sol";
import { FeeOnTransferToken, TestToken } from "./TestTokens.sol";

contract VaultActor {
    receive() external payable {}

    function depositNative(
        PayrailBridgeVault vault,
        bytes32 recipient,
        uint256 amount
    ) external {
        vault.depositNative{value: amount}(recipient);
    }

    function approveAndDeposit(
        PayrailBridgeVault vault,
        TestToken token,
        bytes32 recipient,
        uint256 amount
    ) external {
        token.approve(address(vault), amount);
        vault.depositToken(recipient, address(token), amount);
    }

    function release(
        PayrailBridgeVault vault,
        bytes32 withdrawalId,
        address token,
        address payable recipient,
        uint256 amount
    ) external {
        vault.release(withdrawalId, token, recipient, amount);
    }

    function callRaw(
        address target,
        bytes calldata data,
        uint256 value
    ) external returns (bool success) {
        (success, ) = target.call{value: value}(data);
    }
}

contract PayrailBridgeVaultTest {
    bytes32 private constant PAYRAIL_RECIPIENT = bytes32(uint256(11));

    PayrailBridgeVault private vault;
    VaultActor private depositor;
    VaultActor private releaseAuthority;
    VaultActor private recipient;
    TestToken private token;
    FeeOnTransferToken private feeToken;

    function setUp() public {
        depositor = new VaultActor();
        releaseAuthority = new VaultActor();
        recipient = new VaultActor();
        vault = new PayrailBridgeVault(address(this), address(releaseAuthority));
        token = new TestToken();
        feeToken = new FeeOnTransferToken();
    }

    function testNativeDepositReleaseReplayPauseAndAuthorization() public {
        _fund(address(depositor), 1_000);
        depositor.depositNative(vault, PAYRAIL_RECIPIENT, 1_000);
        require(address(vault).balance == 1_000, "native deposit balance");
        require(vault.depositNonce() == 1, "native deposit nonce");

        (bool direct, ) = address(vault).call{value: 1}("");
        require(!direct, "direct transfer accepted");
        require(address(vault).balance == 1_000, "failed direct transfer mutated balance");

        bool unauthorizedPause = depositor.callRaw(
            address(vault),
            abi.encodeCall(PayrailBridgeVault.setPaused, (true)),
            0
        );
        require(!unauthorizedPause, "unauthorized pause accepted");
        vault.setPaused(true);
        _fund(address(depositor), 1);
        bool pausedDeposit = depositor.callRaw(
            address(depositor),
            abi.encodeCall(VaultActor.depositNative, (vault, PAYRAIL_RECIPIENT, 1)),
            0
        );
        require(!pausedDeposit, "paused deposit accepted");
        vault.setPaused(false);

        bytes32 withdrawal = bytes32(uint256(21));
        uint256 beforeBalance = address(recipient).balance;
        releaseAuthority.release(vault, withdrawal, address(0), payable(recipient), 300);
        require(address(recipient).balance == beforeBalance + 300, "native release amount");
        require(vault.released(withdrawal), "release replay mark missing");
        bool replay = releaseAuthority.callRaw(
            address(releaseAuthority),
            abi.encodeCall(
                VaultActor.release,
                (vault, withdrawal, address(0), payable(recipient), 1)
            ),
            0
        );
        require(!replay, "release replay accepted");
    }

    function testTokenAllowlistExactDepositReleaseAndDelist() public {
        token.mint(address(depositor), 1_000);
        bool beforeAdmission = depositor.callRaw(
            address(depositor),
            abi.encodeCall(
                VaultActor.approveAndDeposit,
                (vault, token, PAYRAIL_RECIPIENT, 500)
            ),
            0
        );
        require(!beforeAdmission, "unlisted token accepted");
        require(token.balanceOf(address(depositor)) == 1_000, "failed deposit mutated sender");

        vault.setTokenAllowed(address(token), true);
        depositor.approveAndDeposit(vault, token, PAYRAIL_RECIPIENT, 500);
        require(token.balanceOf(address(vault)) == 500, "token deposit balance");
        require(vault.depositNonce() == 1, "token deposit nonce");

        releaseAuthority.release(
            vault,
            bytes32(uint256(22)),
            address(token),
            payable(recipient),
            200
        );
        require(token.balanceOf(address(vault)) == 300, "vault release delta");
        require(token.balanceOf(address(recipient)) == 200, "recipient release delta");

        vault.setTokenAllowed(address(token), false);
        bool delistedDeposit = depositor.callRaw(
            address(depositor),
            abi.encodeCall(
                VaultActor.approveAndDeposit,
                (vault, token, PAYRAIL_RECIPIENT, 1)
            ),
            0
        );
        require(!delistedDeposit, "delisted token deposit accepted");
        releaseAuthority.release(
            vault,
            bytes32(uint256(23)),
            address(token),
            payable(recipient),
            100
        );
        require(token.balanceOf(address(vault)) == 200, "delist blocked release");
    }

    function testFeeOnTransferDepositRollsBackCompletely() public {
        feeToken.mint(address(depositor), 1_000);
        vault.setTokenAllowed(address(feeToken), true);
        bool success = depositor.callRaw(
            address(depositor),
            abi.encodeCall(
                VaultActor.approveAndDeposit,
                (vault, TestToken(address(feeToken)), PAYRAIL_RECIPIENT, 100)
            ),
            0
        );
        require(!success, "fee token accepted");
        require(feeToken.balanceOf(address(vault)) == 0, "vault retained failed deposit");
        require(feeToken.balanceOf(address(depositor)) == 1_000, "sender lost failed deposit");
        require(vault.depositNonce() == 0, "failed deposit consumed nonce");
    }

    function testRoleSeparationAndTwoStepOwnership() public {
        (bool sameRelease, ) = address(vault).call(
            abi.encodeCall(PayrailBridgeVault.setReleaseAuthority, (address(this)))
        );
        require(!sameRelease, "owner accepted as release authority");
        (bool releaseAsOwner, ) = address(vault).call(
            abi.encodeCall(PayrailBridgeVault.transferOwnership, (address(releaseAuthority)))
        );
        require(!releaseAsOwner, "release authority accepted as owner");

        vault.transferOwnership(address(recipient));
        bool wrongAccept = depositor.callRaw(
            address(vault),
            abi.encodeCall(PayrailBridgeVault.acceptOwnership, ()),
            0
        );
        require(!wrongAccept, "wrong pending owner accepted");
        bool accepted = recipient.callRaw(
            address(vault),
            abi.encodeCall(PayrailBridgeVault.acceptOwnership, ()),
            0
        );
        require(accepted, "pending owner rejected");
        require(vault.owner() == address(recipient), "owner not transferred");
    }

    function testFuzzNativeDeposit(uint96 rawAmount) public {
        uint256 amount = (uint256(rawAmount) % 1 ether) + 1;
        _fund(address(depositor), amount);
        depositor.depositNative(vault, PAYRAIL_RECIPIENT, amount);
        require(address(vault).balance == amount, "fuzz native balance");
        require(vault.depositNonce() == 1, "fuzz native nonce");
    }

    function testFuzzTokenDepositAndRelease(uint96 rawAmount, uint96 rawRelease) public {
        uint256 amount = uint256(rawAmount) + 1;
        uint256 releaseAmount = (uint256(rawRelease) % amount) + 1;
        token.mint(address(depositor), amount);
        vault.setTokenAllowed(address(token), true);
        depositor.approveAndDeposit(vault, token, PAYRAIL_RECIPIENT, amount);
        releaseAuthority.release(
            vault,
            bytes32(uint256(31)),
            address(token),
            payable(recipient),
            releaseAmount
        );
        require(token.balanceOf(address(vault)) == amount - releaseAmount, "fuzz vault delta");
        require(token.balanceOf(address(recipient)) == releaseAmount, "fuzz recipient delta");
    }

    function _fund(address account, uint256 amount) private {
        (bool success, ) = payable(account).call{value: amount}("");
        require(success, "actor funding failed");
    }
}
