// SPDX-License-Identifier: UNLICENSED
pragma solidity 0.8.24;

interface IERC20 {
    function balanceOf(address account) external view returns (uint256);
    function transfer(address recipient, uint256 amount) external returns (bool);
    function transferFrom(address sender, address recipient, uint256 amount) external returns (bool);
}

/// @notice Custody vault for the explicitly federated Payrail Ethereum bridge.
/// @dev The release authority is expected to be an independently governed
/// multisig or threshold account. This contract is not a trustless light client.
contract PayrailBridgeVault {
    error AlreadyReleased();
    error DirectTransferDisabled();
    error InvalidAmount();
    error InvalidAuthority();
    error InvalidRecipient();
    error InvalidToken();
    error InvalidWithdrawalId();
    error NativeTransferFailed();
    error NotOwner();
    error NotPendingOwner();
    error NotReleaseAuthority();
    error Paused();
    error ReentrantCall();
    error RolesNotSeparated();
    error TokenBalanceMismatch();
    error TokenCallFailed();
    error TokenNotAllowed();

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
    event TokenAdmissionChanged(address indexed token, bool allowed);

    address public owner;
    address public pendingOwner;
    address public releaseAuthority;
    bool public paused;
    uint256 public depositNonce;
    mapping(bytes32 withdrawalId => bool completed) public released;
    mapping(address token => bool allowed) public allowedTokens;

    uint256 private reentrancyState = 1;

    constructor(address initialOwner, address initialReleaseAuthority) {
        if (initialOwner == address(0) || initialReleaseAuthority == address(0)) {
            revert InvalidAuthority();
        }
        if (initialOwner == initialReleaseAuthority) revert RolesNotSeparated();
        owner = initialOwner;
        releaseAuthority = initialReleaseAuthority;
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

    function depositNative(bytes32 payrailRecipient) external payable whenNotPaused {
        if (payrailRecipient == bytes32(0)) revert InvalidRecipient();
        if (msg.value == 0) revert InvalidAmount();
        uint256 nonce = _nextDepositNonce();
        emit Deposited(payrailRecipient, address(0), msg.value, nonce);
    }

    function depositToken(
        bytes32 payrailRecipient,
        address token,
        uint256 amount
    ) external whenNotPaused nonReentrant {
        if (payrailRecipient == bytes32(0)) revert InvalidRecipient();
        if (token == address(0)) revert InvalidToken();
        if (!allowedTokens[token]) revert TokenNotAllowed();
        if (amount == 0) revert InvalidAmount();

        uint256 balanceBefore = _tokenBalance(token, address(this));
        _safeTokenCall(
            token,
            abi.encodeCall(IERC20.transferFrom, (msg.sender, address(this), amount))
        );
        uint256 balanceAfter = _tokenBalance(token, address(this));
        if (balanceAfter < balanceBefore || balanceAfter - balanceBefore != amount) {
            revert TokenBalanceMismatch();
        }

        uint256 nonce = _nextDepositNonce();
        emit Deposited(payrailRecipient, token, amount, nonce);
    }

    function release(
        bytes32 withdrawalId,
        address token,
        address payable recipient,
        uint256 amount
    ) external onlyReleaseAuthority whenNotPaused nonReentrant {
        if (withdrawalId == bytes32(0)) revert InvalidWithdrawalId();
        if (recipient == address(0)) revert InvalidRecipient();
        if (amount == 0) revert InvalidAmount();
        if (released[withdrawalId]) revert AlreadyReleased();

        released[withdrawalId] = true;
        if (token == address(0)) {
            (bool success, ) = recipient.call{value: amount}("");
            if (!success) revert NativeTransferFailed();
        } else {
            uint256 vaultBalanceBefore = _tokenBalance(token, address(this));
            uint256 recipientBalanceBefore = _tokenBalance(token, recipient);
            _safeTokenCall(token, abi.encodeCall(IERC20.transfer, (recipient, amount)));
            uint256 vaultBalanceAfter = _tokenBalance(token, address(this));
            uint256 recipientBalanceAfter = _tokenBalance(token, recipient);
            if (
                vaultBalanceAfter > vaultBalanceBefore ||
                vaultBalanceBefore - vaultBalanceAfter != amount ||
                recipientBalanceAfter < recipientBalanceBefore ||
                recipientBalanceAfter - recipientBalanceBefore != amount
            ) {
                revert TokenBalanceMismatch();
            }
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

    function setTokenAllowed(address token, bool allowed) external onlyOwner {
        if (token == address(0)) revert InvalidToken();
        allowedTokens[token] = allowed;
        emit TokenAdmissionChanged(token, allowed);
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

    function _nextDepositNonce() private returns (uint256 nonce) {
        nonce = depositNonce;
        depositNonce = nonce + 1;
    }

    function _tokenBalance(address token, address account) private view returns (uint256 balance) {
        (bool success, bytes memory result) = token.staticcall(
            abi.encodeCall(IERC20.balanceOf, (account))
        );
        if (!success || result.length != 32) revert TokenCallFailed();
        balance = abi.decode(result, (uint256));
    }

    function _safeTokenCall(address token, bytes memory callData) private {
        (bool success, bytes memory result) = token.call(callData);
        if (!success || (result.length != 0 && (result.length != 32 || !abi.decode(result, (bool))))) {
            revert TokenCallFailed();
        }
    }
}
