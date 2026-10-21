// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.24;

interface IPayrailCustodyToken {
    function balanceOf(address account) external view returns (uint256);
    function transfer(address recipient, uint256 amount) external;
    function transferFrom(address sender, address recipient, uint256 amount) external;
}

/// @notice Single-token custody vault for a federated Payrail bridge.
/// @dev The immutable token boundary prevents governance from silently
/// admitting another asset. Low-level calls support legacy USDT transfers that
/// return no boolean while exact balance checks reject fee-on-transfer tokens.
contract PayrailTokenVault {
    error AlreadyReleased();
    error DirectTransferDisabled();
    error InvalidAmount();
    error InvalidAuthority();
    error InvalidRecipient();
    error InvalidToken();
    error InvalidWithdrawalId();
    error NotOwner();
    error NotPendingOwner();
    error NotReleaseAuthority();
    error Paused();
    error ReentrantCall();
    error RolesNotSeparated();
    error TokenBalanceMismatch();
    error TokenCallFailed();

    event Deposited(
        bytes32 indexed payrailRecipient,
        address indexed token,
        uint256 amount,
        uint256 depositNonce
    );
    event Released(
        bytes32 indexed withdrawalId,
        address indexed token,
        address indexed recipient,
        uint256 amount
    );
    event OwnerTransferStarted(address indexed currentOwner, address indexed pendingOwner);
    event OwnerTransferred(address indexed previousOwner, address indexed newOwner);
    event PauseChanged(bool paused);
    event ReleaseAuthorityChanged(address indexed previousAuthority, address indexed newAuthority);

    address public immutable token;
    address public owner;
    address public pendingOwner;
    address public releaseAuthority;
    bool public paused;
    uint256 public depositNonce;
    mapping(bytes32 withdrawalId => bool completed) public released;

    uint256 private reentrancyState = 1;

    constructor(address custodyToken, address initialOwner, address initialReleaseAuthority) {
        if (custodyToken == address(0) || custodyToken.code.length == 0) revert InvalidToken();
        if (initialOwner == address(0) || initialReleaseAuthority == address(0)) {
            revert InvalidAuthority();
        }
        if (initialOwner == initialReleaseAuthority) revert RolesNotSeparated();
        token = custodyToken;
        owner = initialOwner;
        releaseAuthority = initialReleaseAuthority;
        _tokenBalance(address(this));
    }

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier onlyReleaseAuthority() {
        if (msg.sender != releaseAuthority) revert NotReleaseAuthority();
        _;
    }

    modifier whenNotPaused() {
        if (paused) revert Paused();
        _;
    }

    modifier nonReentrant() {
        if (reentrancyState != 1) revert ReentrantCall();
        reentrancyState = 2;
        _;
        reentrancyState = 1;
    }

    function deposit(bytes32 payrailRecipient, uint256 amount) external whenNotPaused nonReentrant {
        if (payrailRecipient == bytes32(0)) revert InvalidRecipient();
        if (amount == 0) revert InvalidAmount();

        uint256 balanceBefore = _tokenBalance(address(this));
        _safeTokenCall(
            abi.encodeCall(IPayrailCustodyToken.transferFrom, (msg.sender, address(this), amount))
        );
        uint256 balanceAfter = _tokenBalance(address(this));
        if (balanceAfter < balanceBefore || balanceAfter - balanceBefore != amount) {
            revert TokenBalanceMismatch();
        }

        uint256 nonce = depositNonce;
        depositNonce = nonce + 1;
        emit Deposited(payrailRecipient, token, amount, nonce);
    }

    function release(
        bytes32 withdrawalId,
        address recipient,
        uint256 amount
    ) external onlyReleaseAuthority whenNotPaused nonReentrant {
        if (withdrawalId == bytes32(0)) revert InvalidWithdrawalId();
        if (recipient == address(0)) revert InvalidRecipient();
        if (amount == 0) revert InvalidAmount();
        if (released[withdrawalId]) revert AlreadyReleased();

        released[withdrawalId] = true;
        uint256 vaultBalanceBefore = _tokenBalance(address(this));
        uint256 recipientBalanceBefore = _tokenBalance(recipient);
        _safeTokenCall(abi.encodeCall(IPayrailCustodyToken.transfer, (recipient, amount)));
        uint256 vaultBalanceAfter = _tokenBalance(address(this));
        uint256 recipientBalanceAfter = _tokenBalance(recipient);
        if (
            vaultBalanceAfter > vaultBalanceBefore ||
            vaultBalanceBefore - vaultBalanceAfter != amount ||
            recipientBalanceAfter < recipientBalanceBefore ||
            recipientBalanceAfter - recipientBalanceBefore != amount
        ) {
            revert TokenBalanceMismatch();
        }
        emit Released(withdrawalId, token, recipient, amount);
    }

    function setPaused(bool nextPaused) external onlyOwner {
        paused = nextPaused;
        emit PauseChanged(nextPaused);
    }

    function setReleaseAuthority(address nextAuthority) external onlyOwner {
        if (nextAuthority == address(0)) revert InvalidAuthority();
        if (nextAuthority == owner || nextAuthority == pendingOwner) revert RolesNotSeparated();
        address previousAuthority = releaseAuthority;
        releaseAuthority = nextAuthority;
        emit ReleaseAuthorityChanged(previousAuthority, nextAuthority);
    }

    function transferOwnership(address nextOwner) external onlyOwner {
        if (nextOwner == address(0)) revert InvalidAuthority();
        if (nextOwner == releaseAuthority) revert RolesNotSeparated();
        pendingOwner = nextOwner;
        emit OwnerTransferStarted(owner, nextOwner);
    }

    function acceptOwnership() external {
        if (msg.sender != pendingOwner) revert NotPendingOwner();
        address previousOwner = owner;
        owner = msg.sender;
        pendingOwner = address(0);
        emit OwnerTransferred(previousOwner, msg.sender);
    }

    receive() external payable {
        revert DirectTransferDisabled();
    }

    fallback() external payable {
        revert DirectTransferDisabled();
    }

    function _tokenBalance(address account) private view returns (uint256 balance) {
        (bool success, bytes memory result) = token.staticcall(
            abi.encodeCall(IPayrailCustodyToken.balanceOf, (account))
        );
        if (!success || result.length != 32) revert TokenCallFailed();
        balance = abi.decode(result, (uint256));
    }

    function _safeTokenCall(bytes memory callData) private {
        (bool success, bytes memory result) = token.call(callData);
        if (!success || (result.length != 0 && (result.length != 32 || !abi.decode(result, (bool))))) {
            revert TokenCallFailed();
        }
    }
}
