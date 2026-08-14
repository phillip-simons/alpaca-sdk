//! Every wire value of every enum, spelled out once.
//!
//! This is a lock, not a derivation: the string on the left is what goes on the
//! wire, and changing one here is how you would notice you had changed one
//! there. Unrecognised values decode into `Unknown(String)`, so a typo in a
//! variant does not fail loudly at runtime — it quietly stops matching, which is
//! precisely the failure a table like this catches.
//!
//! Adding a value Alpaca has started sending means adding a line here too.
//! Removing one is almost always wrong: `just enums-drift` lists the values the
//! specs no longer document, and the API still serves several of them.

// `broker` implies `trading`, so naming the two surfaces this file actually
// draws enums from is enough. Without this the file is a compile error under
// every reduced feature set -- which `just features` could not see, because
// `cargo check` alone never builds test targets.
#![cfg(all(feature = "broker", feature = "data"))]
#![allow(clippy::too_many_lines)]

use alpaca_sdk::broker::{
    ACHRelationshipStatus, AccountEntities, AccountSubType, AccountType, AgreementType,
    BankAccountType, BankStatus, CIPApprovalStatus, CIPProvider, CIPResult, CIPStatus,
    CalendarSubType, ClearingBroker, DocumentType, DriftBandSubType, EmploymentStatus,
    FeePaymentMethod, FundingSource, IdentifierType, JournalEntryType, JournalStatus,
    PortfolioStatus, RebalancingConditionsType, RunInitiatedFrom, RunStatus, RunType, TaxIdType,
    TradeDocumentSubType, TradeDocumentType, TransferDirection, TransferStatus, TransferTiming,
    TransferType, UploadDocumentMimeType, UploadDocumentSubType, VisaType, WeightType,
};
use alpaca_sdk::data::{
    Adjustment, CorporateActionsType, CryptoFeed, DataFeed, Exchange, MarketType, MostActivesBy,
    NewsImageSize, OptionsFeed,
};
use alpaca_sdk::trading::{
    AccountStatus, ActivityCategory, ActivityType, AssetClass, AssetExchange, AssetStatus,
    CorporateActionDateType, CorporateActionSubType, CorporateActionType, DTBPCheck, ExerciseStyle,
    NonTradeActivityStatus, OrderClass, OrderSide, OrderStatus, OrderType, PDTCheck,
    PositionIntent, PositionSide, QueryOrderStatus, TimeInForce, TradeActivityType,
    TradeConfirmationEmail, TradeEvent,
};
use alpaca_sdk::types::ContractType;

/// Asserts the known wire values and that each one round-trips.
macro_rules! assert_wire_values {
    ($ty:ty, $values:expr) => {{
        assert_eq!(<$ty>::WIRE_VALUES, $values, stringify!($ty));
        for value in <$ty>::WIRE_VALUES {
            let parsed = <$ty>::from(*value);
            assert!(!parsed.is_unknown(), "{}: {value}", stringify!($ty));
            assert_eq!(parsed.as_str(), *value, stringify!($ty));
        }
    }};
}

#[test]
fn trading_activity_type_wire_values() {
    assert_wire_values!(
        ActivityType,
        [
            "FILL", "ACATC", "ACATS", "CFEE", "CGD", "CIL", "CSD", "CSW", "DIV", "DIVCGL",
            "DIVCGS", "DIVFEE", "DIVFT", "DIVNRA", "DIVROC", "DIVTW", "DIVTXEX", "DIVWH", "EXTRD",
            "FEE", "FOPT", "FXTRD", "INT", "INTNRA", "INTPNL", "INTTW", "JNL", "JNLC", "JNLS",
            "MA", "MEM", "MISC", "NC", "OCT", "OPASN", "OPCA", "OPCSH", "OPEXC", "OPEXP", "OPTRD",
            "PTC", "PTR", "REORG", "SPIN", "SPLIT", "SWP", "TRANS", "VOF", "WH"
        ]
    );
}

#[test]
fn trading_trade_activity_type_wire_values() {
    assert_wire_values!(TradeActivityType, ["partial_fill", "fill"]);
}

#[test]
fn trading_non_trade_activity_status_wire_values() {
    assert_wire_values!(NonTradeActivityStatus, ["executed", "correct", "canceled"]);
}

#[test]
fn trading_order_class_wire_values() {
    assert_wire_values!(OrderClass, ["simple", "mleg", "bracket", "oco", "oto"]);
}

#[test]
fn trading_order_type_wire_values() {
    assert_wire_values!(
        OrderType,
        ["market", "limit", "stop", "stop_limit", "trailing_stop"]
    );
}

#[test]
fn trading_order_side_wire_values() {
    assert_wire_values!(
        OrderSide,
        [
            "buy",
            "sell",
            "buy_minus",
            "sell_plus",
            "sell_short",
            "sell_short_exempt",
            "undisclosed",
            "cross",
            "cross_short"
        ]
    );
}

#[test]
fn trading_order_status_wire_values() {
    assert_wire_values!(
        OrderStatus,
        [
            "new",
            "partially_filled",
            "filled",
            "done_for_day",
            "canceled",
            "expired",
            "replaced",
            "pending_cancel",
            "pending_replace",
            "pending_review",
            "accepted",
            "pending_new",
            "accepted_for_bidding",
            "stopped",
            "rejected",
            "suspended",
            "calculated",
            "held"
        ]
    );
}

#[test]
fn trading_asset_class_wire_values() {
    assert_wire_values!(
        AssetClass,
        [
            "us_equity",
            "us_option",
            "crypto",
            "crypto_perp",
            "us_equity_chain",
            "us_index",
            "global_equity",
            "treasury",
            "corporate",
            "ipo"
        ]
    );
}

#[test]
fn trading_asset_status_wire_values() {
    assert_wire_values!(AssetStatus, ["active", "inactive"]);
}

#[test]
fn trading_asset_exchange_wire_values() {
    assert_wire_values!(
        AssetExchange,
        [
            "AMEX", "ARCA", "ASCX", "BATS", "NYSE", "NASDAQ", "NYSEARCA", "FTXU", "CBSE", "GNSS",
            "ERSX", "OTC", "CRYPTO", ""
        ]
    );
}

#[test]
fn trading_position_side_wire_values() {
    assert_wire_values!(PositionSide, ["short", "long"]);
}

#[test]
fn trading_time_in_force_wire_values() {
    assert_wire_values!(TimeInForce, ["day", "gtc", "opg", "cls", "ioc", "fok"]);
}

#[test]
fn trading_corporate_action_type_wire_values() {
    assert_wire_values!(
        CorporateActionType,
        ["dividend", "merger", "spinoff", "split"]
    );
}

#[test]
fn trading_corporate_action_sub_type_wire_values() {
    assert_wire_values!(
        CorporateActionSubType,
        [
            "cash",
            "stock",
            "merger_update",
            "merger_completion",
            "spinoff",
            "stock_split",
            "unit_split",
            "reverse_split",
            "recapitalization"
        ]
    );
}

#[test]
fn trading_account_status_wire_values() {
    assert_wire_values!(
        AccountStatus,
        [
            "ACCOUNT_CLOSED",
            "ACCOUNT_CLOSED_PENDING",
            "ACCOUNT_UPDATED",
            "ACTION_REQUIRED",
            "ACTIVE",
            "AML_REVIEW",
            "APPROVAL_PENDING",
            "APPROVED",
            "DISABLED",
            "DISABLE_PENDING",
            "EDITED",
            "INACTIVE",
            "KYC_SUBMITTED",
            "LIMITED",
            "ONBOARDING",
            "PAPER_ONLY",
            "REAPPROVAL_PENDING",
            "REJECTED",
            "RESUBMITTED",
            "SIGNED_UP",
            "SUBMISSION_FAILED",
            "SUBMITTED"
        ]
    );
}

#[test]
fn trading_corporate_action_date_type_wire_values() {
    assert_wire_values!(
        CorporateActionDateType,
        ["declaration_date", "ex_date", "record_date", "payable_date"]
    );
}

#[test]
fn trading_trade_event_wire_values() {
    assert_wire_values!(
        TradeEvent,
        [
            "accepted",
            "canceled",
            "expired",
            "fill",
            "new",
            "partial_fill",
            "pending_cancel",
            "pending_new",
            "pending_replace",
            "rejected",
            "replaced",
            "restated"
        ]
    );
}

#[test]
fn trading_query_order_status_wire_values() {
    assert_wire_values!(QueryOrderStatus, ["open", "closed", "all"]);
}

#[test]
fn trading_d_t_b_p_check_wire_values() {
    assert_wire_values!(DTBPCheck, ["both", "entry", "exit"]);
}

#[test]
fn trading_p_d_t_check_wire_values() {
    assert_wire_values!(PDTCheck, ["both", "entry", "exit"]);
}

#[test]
fn trading_trade_confirmation_email_wire_values() {
    assert_wire_values!(TradeConfirmationEmail, ["all", "none"]);
}

#[test]
fn trading_exercise_style_wire_values() {
    assert_wire_values!(ExerciseStyle, ["american", "european"]);
}

#[test]
fn trading_activity_category_wire_values() {
    assert_wire_values!(ActivityCategory, ["trade_activity", "non_trade_activity"]);
}

#[test]
fn trading_position_intent_wire_values() {
    assert_wire_values!(
        PositionIntent,
        [
            "buy_to_open",
            "buy_to_close",
            "sell_to_open",
            "sell_to_close"
        ]
    );
}

#[test]
fn data_exchange_wire_values() {
    assert_wire_values!(
        Exchange,
        [
            "Z", "I", "M", "U", "L", "W", "X", "B", "D", "J", "P", "Q", "S", "V", "A", "E", "N",
            "T", "Y", "C", "H", "K"
        ]
    );
}

#[test]
fn data_data_feed_wire_values() {
    assert_wire_values!(
        DataFeed,
        ["iex", "sip", "delayed_sip", "otc", "boats", "overnight"]
    );
}

#[test]
fn data_adjustment_wire_values() {
    assert_wire_values!(Adjustment, ["raw", "split", "dividend", "all"]);
}

#[test]
fn data_crypto_feed_wire_values() {
    assert_wire_values!(CryptoFeed, ["us"]);
}

#[test]
fn data_options_feed_wire_values() {
    assert_wire_values!(OptionsFeed, ["opra", "indicative"]);
}

#[test]
fn data_most_actives_by_wire_values() {
    assert_wire_values!(MostActivesBy, ["volume", "trades"]);
}

#[test]
fn data_market_type_wire_values() {
    assert_wire_values!(MarketType, ["stocks", "crypto"]);
}

#[test]
fn data_news_image_size_wire_values() {
    assert_wire_values!(NewsImageSize, ["thumb", "small", "large"]);
}

#[test]
fn data_corporate_actions_type_wire_values() {
    assert_wire_values!(
        CorporateActionsType,
        [
            "reverse_split",
            "forward_split",
            "unit_split",
            "cash_dividend",
            "stock_dividend",
            "spin_off",
            "cash_merger",
            "stock_merger",
            "stock_and_cash_merger",
            "redemption",
            "name_change",
            "worthless_removal",
            "rights_distribution"
        ]
    );
}

#[test]
fn broker_account_sub_type_wire_values() {
    assert_wire_values!(AccountSubType, ["traditional", "roth"]);
}

#[test]
fn broker_account_type_wire_values() {
    assert_wire_values!(
        AccountType,
        ["trading", "custodial", "donor_advised", "ira", "hsa"]
    );
}

#[test]
fn broker_tax_id_type_wire_values() {
    assert_wire_values!(
        TaxIdType,
        [
            "USA_SSN",
            "USA_ITIN",
            "ARG_AR_CUIT",
            "AUS_TFN",
            "AUS_ABN",
            "BOL_NIT",
            "BRA_CPF",
            "CHL_RUT",
            "COL_NIT",
            "CRI_NITE",
            "DEU_TAX_ID",
            "DOM_RNC",
            "ECU_RUC",
            "FRA_SPI",
            "GBR_UTR",
            "GBR_NINO",
            "GTM_NIT",
            "HND_RTN",
            "HUN_TIN",
            "IDN_KTP",
            "IND_PAN",
            "ISR_TAX_ID",
            "ITA_TAX_ID",
            "JPN_TAX_ID",
            "MEX_RFC",
            "NIC_RUC",
            "NLD_TIN",
            "PAN_RUC",
            "PER_RUC",
            "PRY_RUC",
            "SGP_NRIC",
            "SGP_FIN",
            "SGP_ASGD",
            "SGP_ITR",
            "SLV_NIT",
            "SWE_TAX_ID",
            "URY_RUT",
            "VEN_RIF",
            "NATIONAL_ID",
            "PASSPORT",
            "PERMANENT_RESIDENT",
            "DRIVER_LICENSE",
            "OTHER_GOV_ID",
            "NOT_SPECIFIED"
        ]
    );
}

#[test]
fn broker_visa_type_wire_values() {
    assert_wire_values!(
        VisaType,
        [
            "B1", "B2", "DACA", "E1", "E2", "E3", "F1", "G4", "H1B", "J1", "L1", "OTHER", "O1",
            "TN1"
        ]
    );
}

#[test]
fn broker_funding_source_wire_values() {
    assert_wire_values!(
        FundingSource,
        [
            "employment_income",
            "investments",
            "inheritance",
            "business_income",
            "savings",
            "family"
        ]
    );
}

#[test]
fn broker_employment_status_wire_values() {
    assert_wire_values!(
        EmploymentStatus,
        ["UNEMPLOYED", "EMPLOYED", "STUDENT", "RETIRED"]
    );
}

#[test]
fn broker_agreement_type_wire_values() {
    assert_wire_values!(
        AgreementType,
        [
            "margin_agreement",
            "account_agreement",
            "customer_agreement",
            "crypto_agreement",
            "options_agreement",
            "custodial_customer_agreement"
        ]
    );
}

#[test]
fn broker_document_type_wire_values() {
    assert_wire_values!(
        DocumentType,
        [
            "identity_verification",
            "address_verification",
            "date_of_birth_verification",
            "tax_id_verification",
            "account_approval_letter",
            "limited_trading_authorization",
            "w8ben",
            "social_security_number_verification",
            "",
            "cip_result",
            "other"
        ]
    );
}

#[test]
fn broker_account_entities_wire_values() {
    assert_wire_values!(
        AccountEntities,
        [
            "contact",
            "identity",
            "disclosures",
            "agreements",
            "documents",
            "trusted_contact",
            "trading_configurations"
        ]
    );
}

#[test]
fn broker_clearing_broker_wire_values() {
    assert_wire_values!(
        ClearingBroker,
        [
            "APEX",
            "ETC",
            "IC",
            "VELOX",
            "VISION",
            "SELF",
            "ALPACA_APCA"
        ]
    );
}

#[test]
fn broker_c_i_p_provider_wire_values() {
    assert_wire_values!(
        CIPProvider,
        ["alloy", "trulioo", "onfido", "veriff", "jumio", "getmati"]
    );
}

#[test]
fn broker_c_i_p_status_wire_values() {
    assert_wire_values!(CIPStatus, ["complete", "withdrawn"]);
}

#[test]
fn broker_c_i_p_result_wire_values() {
    assert_wire_values!(CIPResult, ["clear", "consider"]);
}

#[test]
fn broker_c_i_p_approval_status_wire_values() {
    assert_wire_values!(CIPApprovalStatus, ["approved", "rejected"]);
}

#[test]
fn broker_trade_document_type_wire_values() {
    assert_wire_values!(
        TradeDocumentType,
        [
            "account_statement",
            "trade_confirmation",
            "trade_confirmation_json",
            "tax_statement",
            "account_application",
            "tax_1099_b_details",
            "tax_1099_b_form",
            "tax_1099_div_details",
            "tax_1099_div_form",
            "tax_1099_int_details",
            "tax_1099_int_form",
            "tax_w8"
        ]
    );
}

#[test]
fn broker_trade_document_sub_type_wire_values() {
    assert_wire_values!(
        TradeDocumentSubType,
        ["1099-Comp", "1042-S", "480.6", "courtesy_statement"]
    );
}

#[test]
fn broker_upload_document_sub_type_wire_values() {
    assert_wire_values!(
        UploadDocumentSubType,
        ["Account Application", "Form W-8BEN", "passport"]
    );
}

#[test]
fn broker_upload_document_mime_type_wire_values() {
    assert_wire_values!(
        UploadDocumentMimeType,
        [
            "application/pdf",
            "image/png",
            "image/jpeg",
            "application/json"
        ]
    );
}

#[test]
fn broker_a_c_h_relationship_status_wire_values() {
    assert_wire_values!(ACHRelationshipStatus, ["QUEUED", "APPROVED", "PENDING"]);
}

#[test]
fn broker_bank_account_type_wire_values() {
    assert_wire_values!(BankAccountType, ["CHECKING", "SAVINGS", ""]);
}

#[test]
fn broker_identifier_type_wire_values() {
    assert_wire_values!(IdentifierType, ["ABA", "BIC"]);
}

#[test]
fn broker_bank_status_wire_values() {
    assert_wire_values!(
        BankStatus,
        ["QUEUED", "SENT_TO_CLEARING", "APPROVED", "CANCELED"]
    );
}

#[test]
fn broker_transfer_type_wire_values() {
    assert_wire_values!(TransferType, ["ach", "wire"]);
}

#[test]
fn broker_transfer_status_wire_values() {
    assert_wire_values!(
        TransferStatus,
        [
            "QUEUED",
            "APPROVAL_PENDING",
            "PENDING",
            "SENT_TO_CLEARING",
            "REJECTED",
            "CANCELED",
            "APPROVED",
            "SETTLED",
            "COMPLETE",
            "RETURNED"
        ]
    );
}

#[test]
fn broker_transfer_direction_wire_values() {
    assert_wire_values!(TransferDirection, ["INCOMING", "OUTGOING"]);
}

#[test]
fn broker_transfer_timing_wire_values() {
    assert_wire_values!(TransferTiming, ["immediate"]);
}

#[test]
fn broker_fee_payment_method_wire_values() {
    assert_wire_values!(FeePaymentMethod, ["user", "invoice"]);
}

#[test]
fn broker_journal_entry_type_wire_values() {
    assert_wire_values!(JournalEntryType, ["JNLC", "JNLS"]);
}

#[test]
fn broker_journal_status_wire_values() {
    assert_wire_values!(
        JournalStatus,
        [
            "queued",
            "sent_to_clearing",
            "pending",
            "executed",
            "activity_created",
            "rejected",
            "canceled",
            "refused",
            "correct",
            "deleted"
        ]
    );
}

#[test]
fn broker_portfolio_status_wire_values() {
    assert_wire_values!(PortfolioStatus, ["active", "inactive", "needs_adjustment"]);
}

#[test]
fn broker_weight_type_wire_values() {
    assert_wire_values!(WeightType, ["cash", "asset"]);
}

#[test]
fn broker_rebalancing_conditions_type_wire_values() {
    assert_wire_values!(RebalancingConditionsType, ["drift_band", "calendar"]);
}

#[test]
fn broker_drift_band_sub_type_wire_values() {
    assert_wire_values!(DriftBandSubType, ["absolute", "relative"]);
}

#[test]
fn broker_calendar_sub_type_wire_values() {
    assert_wire_values!(
        CalendarSubType,
        ["weekly", "monthly", "quarterly", "annually"]
    );
}

#[test]
fn broker_run_type_wire_values() {
    assert_wire_values!(RunType, ["full_rebalance", "invest_cash"]);
}

#[test]
fn broker_run_initiated_from_wire_values() {
    assert_wire_values!(RunInitiatedFrom, ["system", "api"]);
}

#[test]
fn broker_run_status_wire_values() {
    assert_wire_values!(
        RunStatus,
        [
            "QUEUED",
            "IN_PROGRESS",
            "CANCELED",
            "CANCELED_MID_RUN",
            "ERROR",
            "TIMEOUT",
            "COMPLETED_SUCCESS",
            "COMPLETED_ADJUSTED"
        ]
    );
}

#[test]
fn types_contract_type_wire_values() {
    assert_wire_values!(ContractType, ["call", "put"]);
}
