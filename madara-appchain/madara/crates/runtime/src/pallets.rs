//! Configuration of the pallets used in the runtime.
//! The pallets used in the runtime are configured here.
//! This file is used to generate the `construct_runtime!` macro.
pub use frame_support::traits::{
    ConstBool, ConstU128, ConstU32, ConstU64, ConstU8, KeyOwnerProofSystem, OnTimestampSet, Randomness, StorageInfo,
};
pub use frame_support::weights::constants::{
    BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight, WEIGHT_REF_TIME_PER_SECOND,
};
pub use frame_support::weights::{IdentityFee, Weight};
pub use frame_support::{construct_runtime, parameter_types, StorageValue};
pub use frame_system::Call as SystemCall;
pub use mp_chain_id::MADARA_CHAIN_ID;
use mp_fee::ResourcePrice;
pub use mp_program_hash::SN_OS_PROGRAM_HASH;
/// Import the StarkNet pallet.
pub use pallet_starknet;
pub use pallet_timestamp::Call as TimestampCall;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_runtime::traits::{AccountIdLookup, BlakeTwo256};
#[cfg(any(feature = "std", test))]
pub use sp_runtime::BuildStorage;
pub use sp_runtime::{Perbill, Permill};
use sp_std::marker::PhantomData;

use crate::*;

// Configure FRAME pallets to include in runtime.

// --------------------------------------
// CUSTOM PALLETS
// --------------------------------------

/// Configure the Starknet pallet in pallets/starknet.
impl pallet_starknet::Config for Runtime {
    type SystemHash = StarknetHasher;
    type TimestampProvider = Timestamp;
    type UnsignedPriority = UnsignedPriority;
    type TransactionLongevity = TransactionLongevity;
    #[cfg(not(feature = "disable-transaction-fee"))]
    type DisableTransactionFee = ConstBool<false>;
    #[cfg(feature = "disable-transaction-fee")]
    type DisableTransactionFee = ConstBool<true>;
    type DisableNonceValidation = ConstBool<false>;
    type InvokeTxMaxNSteps = InvokeTxMaxNSteps;
    type ValidateMaxNSteps = ValidateMaxNSteps;
    type ProtocolVersion = ProtocolVersion;
    type ChainId = ChainId;
    type MaxRecursionDepth = MaxRecursionDepth;
    type ProgramHash = ProgramHash;
    type L1GasPrice = L1GasPrice;
}

/// --------------------------------------
/// FRAME SYSTEM PALLET
/// --------------------------------------

/// Configuration of `frame_system` pallet.
impl frame_system::Config for Runtime {
    /// The basic call filter to use in dispatchable.
    type BaseCallFilter = frame_support::traits::Everything;
    /// Block & extrinsics weights: base values and limits.
    type BlockWeights = BlockWeights;
    /// The maximum length of a block (in bytes).
    type BlockLength = BlockLength;
    /// The identifier used to distinguish between accounts.
    type AccountId = AccountId;
    /// The aggregated dispatch type that is available for extrinsics.
    type RuntimeCall = RuntimeCall;
    /// The lookup mechanism to get account ID from whatever is passed in dispatchers.
    type Lookup = AccountIdLookup<AccountId, ()>;
    /// The index type for storing how many extrinsics an account has signed.
    type Nonce = Index;
    /// The type for hashing blocks and tries.
    type Hash = Hash;
    /// The hashing algorithm used.
    type Hashing = BlakeTwo256;
    /// The Block type.
    type Block = Block;
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    /// The ubiquitous origin type.
    type RuntimeOrigin = RuntimeOrigin;
    /// Maximum number of block number to block hash mappings to keep (oldest pruned first).
    type BlockHashCount = BlockHashCount;
    /// The weight of database operations that the runtime can invoke.
    type DbWeight = RocksDbWeight;
    /// Version of the runtime.
    type Version = Version;
    /// Converts a module to the index of the module in `construct_runtime!`.
    ///
    /// This type is being generated by `construct_runtime!`.
    type PalletInfo = PalletInfo;
    /// What to do if a new account is created.
    type OnNewAccount = ();
    /// What to do if an account is fully reaped from the system.
    type OnKilledAccount = ();
    /// The data to be stored in an account.
    type AccountData = ();
    /// Weight information for the extrinsics of this pallet.
    type SystemWeightInfo = ();
    /// This is used as an identifier of the chain. 42 is the generic substrate prefix.
    type SS58Prefix = SS58Prefix;
    /// The set code logic, just the default since we're not a parachain.
    type OnSetCode = ();
    type MaxConsumers = frame_support::traits::ConstU32<16>;
}

// --------------------------------------
// CONSENSUS RELATED FRAME PALLETS
// --------------------------------------
// Notes:
// Aura is the consensus algorithm used for block production.
// Grandpa is the consensus algorithm used for block finalization.
// We want to support multiple flavors of consensus algorithms.
// Specifically we want to implement some proposals defined in the Starknet community forum.
// For more information see: https://community.starknet.io/t/starknet-decentralized-protocol-i-introduction/2671
// You can also follow this issue on github: https://github.com/keep-starknet-strange/madara/issues/83

/// Authority-based consensus protocol used for block production.
/// TODO: Comment and explain the rationale behind the configuration items.
impl pallet_aura::Config for Runtime {
    type AuthorityId = AuraId;
    type DisabledValidators = ();
    type MaxAuthorities = ConstU32<32>;
    type AllowMultipleBlocksPerSlot = ConstBool<false>;
}

/// Deterministic finality mechanism used for block finalization.
/// TODO: Comment and explain the rationale behind the configuration items.
impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;

    type WeightInfo = ();
    type MaxAuthorities = ConstU32<32>;
    type MaxSetIdSessionEntries = ConstU64<0>;
    type MaxNominators = ConstU32<1000>;

    type KeyOwnerProof = sp_core::Void;
    type EquivocationReportSystem = ();
}

/// --------------------------------------
/// OTHER 3RD PARTY FRAME PALLETS
/// --------------------------------------

/// Timestamp manipulation.
/// For instance, we need it to set the timestamp of the Starknet block.
impl pallet_timestamp::Config for Runtime {
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = u64;
    type OnTimestampSet = ConsensusOnTimestampSet<Self>;
    type MinimumPeriod = ConstU64<{ SLOT_DURATION / 2 }>;
    type WeightInfo = ();
}

parameter_types! {
    pub const UnsignedPriority: u64 = 1 << 20;
    pub const TransactionLongevity: u64 = u64::MAX;
    pub const InvokeTxMaxNSteps: u32 = 1_000_000;
    pub const ValidateMaxNSteps: u32 = 1_000_000;
    pub const ProtocolVersion: u8 = 0;
    pub const ChainId: Felt252Wrapper = MADARA_CHAIN_ID;
    pub const MaxRecursionDepth: u32 = 50;
    pub const ProgramHash: Felt252Wrapper = SN_OS_PROGRAM_HASH;
    pub const L1GasPrice: ResourcePrice = ResourcePrice { price_in_strk: None, price_in_wei: 10 };
}

/// Implement the OnTimestampSet trait to override the default Aura.
/// This is needed to suppress Aura validations in case of non-default sealing.
pub struct ConsensusOnTimestampSet<T>(PhantomData<T>);
impl<T: pallet_aura::Config> OnTimestampSet<T::Moment> for ConsensusOnTimestampSet<T> {
    fn on_timestamp_set(moment: T::Moment) {
        if Sealing::get() != SealingMode::Default {
            return;
        }
        <pallet_aura::Pallet<T> as OnTimestampSet<T::Moment>>::on_timestamp_set(moment)
    }
}
