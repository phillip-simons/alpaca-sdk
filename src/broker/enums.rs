//! The string enums the broker API accepts and sends.
//!
//! The broker's vocabularies do not line up with the trading API's, and each
//! divergence is a separate type rather than a shared one — `COMPLETE` against
//! `COMPLETED`, `incoming` against `INCOMING`, `ASC` against `asc`. Unifying
//! them is exactly what would break decoding.
//!
//! Hand-written `impl` blocks belong in the sibling `enums_ext.rs`.

use crate::types::wire::wire_enum;

/// The sub type of account
/// IRA Account only
///
/// See <https://docs.alpaca.markets/reference/createaccount>.
#[wire_enum]
pub enum AccountSubType {
    /// `traditional`
    #[wire = "traditional"]
    Traditional,
    /// `roth`
    #[wire = "roth"]
    Roth,
}

/// The type of account
///
/// See <https://docs.alpaca.markets/reference/createaccount>.
#[wire_enum]
pub enum AccountType {
    /// `trading`
    #[wire = "trading"]
    Trading,
    /// `custodial`
    #[wire = "custodial"]
    Custodial,
    /// `donor_advised`
    #[wire = "donor_advised"]
    DonorAdvised,
    /// `ira`
    #[wire = "ira"]
    Ira,
    /// `hsa`
    #[wire = "hsa"]
    Hsa,
}

/// The various country specific tax identification numbers
///
/// See <https://docs.alpaca.markets/docs/broker/api-references/accounts/accounts/#tax-id-type>.
#[wire_enum]
pub enum TaxIdType {
    /// `USA_SSN`
    #[wire = "USA_SSN"]
    UsaSsn,
    /// `USA_ITIN`
    #[wire = "USA_ITIN"]
    UsaItin,
    /// `ARG_AR_CUIT`
    #[wire = "ARG_AR_CUIT"]
    ArgArCuit,
    /// `AUS_TFN`
    #[wire = "AUS_TFN"]
    AusTfn,
    /// `AUS_ABN`
    #[wire = "AUS_ABN"]
    AusAbn,
    /// `BOL_NIT`
    #[wire = "BOL_NIT"]
    BolNit,
    /// `BRA_CPF`
    #[wire = "BRA_CPF"]
    BraCpf,
    /// `CHL_RUT`
    #[wire = "CHL_RUT"]
    ChlRut,
    /// `COL_NIT`
    #[wire = "COL_NIT"]
    ColNit,
    /// `CRI_NITE`
    #[wire = "CRI_NITE"]
    CriNite,
    /// `DEU_TAX_ID`
    #[wire = "DEU_TAX_ID"]
    DeuTaxId,
    /// `DOM_RNC`
    #[wire = "DOM_RNC"]
    DomRnc,
    /// `ECU_RUC`
    #[wire = "ECU_RUC"]
    EcuRuc,
    /// `FRA_SPI`
    #[wire = "FRA_SPI"]
    FraSpi,
    /// `GBR_UTR`
    #[wire = "GBR_UTR"]
    GbrUtr,
    /// `GBR_NINO`
    #[wire = "GBR_NINO"]
    GbrNino,
    /// `GTM_NIT`
    #[wire = "GTM_NIT"]
    GtmNit,
    /// `HND_RTN`
    #[wire = "HND_RTN"]
    HndRtn,
    /// `HUN_TIN`
    #[wire = "HUN_TIN"]
    HunTin,
    /// `IDN_KTP`
    #[wire = "IDN_KTP"]
    IdnKtp,
    /// `IND_PAN`
    #[wire = "IND_PAN"]
    IndPan,
    /// `ISR_TAX_ID`
    #[wire = "ISR_TAX_ID"]
    IsrTaxId,
    /// `ITA_TAX_ID`
    #[wire = "ITA_TAX_ID"]
    ItaTaxId,
    /// `JPN_TAX_ID`
    #[wire = "JPN_TAX_ID"]
    JpnTaxId,
    /// `MEX_RFC`
    #[wire = "MEX_RFC"]
    MexRfc,
    /// `NIC_RUC`
    #[wire = "NIC_RUC"]
    NicRuc,
    /// `NLD_TIN`
    #[wire = "NLD_TIN"]
    NldTin,
    /// `PAN_RUC`
    #[wire = "PAN_RUC"]
    PanRuc,
    /// `PER_RUC`
    #[wire = "PER_RUC"]
    PerRuc,
    /// `PRY_RUC`
    #[wire = "PRY_RUC"]
    PryRuc,
    /// `SGP_NRIC`
    #[wire = "SGP_NRIC"]
    SgpNric,
    /// `SGP_FIN`
    #[wire = "SGP_FIN"]
    SgpFin,
    /// `SGP_ASGD`
    #[wire = "SGP_ASGD"]
    SgpAsgd,
    /// `SGP_ITR`
    #[wire = "SGP_ITR"]
    SgpItr,
    /// `SLV_NIT`
    #[wire = "SLV_NIT"]
    SlvNit,
    /// `SWE_TAX_ID`
    #[wire = "SWE_TAX_ID"]
    SweTaxId,
    /// `URY_RUT`
    #[wire = "URY_RUT"]
    UryRut,
    /// `VEN_RIF`
    #[wire = "VEN_RIF"]
    VenRif,
    /// `NATIONAL_ID`
    #[wire = "NATIONAL_ID"]
    NationalId,
    /// `PASSPORT`
    #[wire = "PASSPORT"]
    Passport,
    /// `PERMANENT_RESIDENT`
    #[wire = "PERMANENT_RESIDENT"]
    PermanentResident,
    /// `DRIVER_LICENSE`
    #[wire = "DRIVER_LICENSE"]
    DriverLicense,
    /// `OTHER_GOV_ID`
    #[wire = "OTHER_GOV_ID"]
    OtherGovId,
    /// `NOT_SPECIFIED`
    #[wire = "NOT_SPECIFIED"]
    NotSpecified,
}

/// In addition to the following USA visa categories, we accept any sub visas of the list below.
/// Sub visas must be passed in according to their parent category.
/// Note that United States green card holders are considered permanent residents and should not pass in a visa type.
///
/// Please feel free to reach out to Alpaca if you need other tax ID types.
///
/// See <https://docs.alpaca.markets/docs/broker/api-references/accounts/accounts/#visa-type>.
#[wire_enum]
pub enum VisaType {
    /// `B1`
    #[wire = "B1"]
    B1,
    /// `B2`
    #[wire = "B2"]
    B2,
    /// `DACA`
    #[wire = "DACA"]
    Daca,
    /// `E1`
    #[wire = "E1"]
    E1,
    /// `E2`
    #[wire = "E2"]
    E2,
    /// `E3`
    #[wire = "E3"]
    E3,
    /// `F1`
    #[wire = "F1"]
    F1,
    /// `G4`
    #[wire = "G4"]
    G4,
    /// `H1B`
    #[wire = "H1B"]
    H1b,
    /// `J1`
    #[wire = "J1"]
    J1,
    /// `L1`
    #[wire = "L1"]
    L1,
    /// `OTHER`
    #[wire = "OTHER"]
    Other,
    /// `O1`
    #[wire = "O1"]
    O1,
    /// `TN1`
    #[wire = "TN1"]
    Tn1,
}

/// Various sources of funding for brokerage accounts.
///
/// See <https://docs.alpaca.markets/docs/broker/api-references/accounts/accounts/#funding-source>.
#[wire_enum]
pub enum FundingSource {
    /// `employment_income`
    #[wire = "employment_income"]
    EmploymentIncome,
    /// `investments`
    #[wire = "investments"]
    Investments,
    /// `inheritance`
    #[wire = "inheritance"]
    Inheritance,
    /// `business_income`
    #[wire = "business_income"]
    BusinessIncome,
    /// `savings`
    #[wire = "savings"]
    Savings,
    /// `family`
    #[wire = "family"]
    Family,
}

/// The possible employment statuses of the user
///
/// See <https://docs.alpaca.markets/docs/broker/api-references/accounts/accounts/#employment-status>.
#[wire_enum]
pub enum EmploymentStatus {
    /// `UNEMPLOYED`
    #[wire = "UNEMPLOYED"]
    Unemployed,
    /// `EMPLOYED`
    #[wire = "EMPLOYED"]
    Employed,
    /// `STUDENT`
    #[wire = "STUDENT"]
    Student,
    /// `RETIRED`
    #[wire = "RETIRED"]
    Retired,
}

/// The types of agreements that are to be signed by the user
///
/// See <https://docs.alpaca.markets/reference/createaccount>.
#[wire_enum]
pub enum AgreementType {
    /// `margin_agreement`
    #[wire = "margin_agreement"]
    Margin,
    /// `account_agreement`
    #[wire = "account_agreement"]
    Account,
    /// `customer_agreement`
    #[wire = "customer_agreement"]
    Customer,
    /// `crypto_agreement`
    #[wire = "crypto_agreement"]
    Crypto,
    /// `options_agreement`
    #[wire = "options_agreement"]
    Options,
    /// `custodial_customer_agreement`
    #[wire = "custodial_customer_agreement"]
    CustodialCustomer,
}

/// The kind of document being uploaded during account onboarding.
///
/// Distinct from [`TradeDocumentType`], which classifies documents Alpaca
/// *generates* for an account — statements, confirmations, tax forms.
///
/// See the [document type reference][types] and the [upload
/// enumeration][upload].
///
/// [types]: https://docs.alpaca.markets/docs/broker/api-references/accounts/accounts/#document-type
/// [upload]: https://docs.alpaca.markets/docs/api-references/broker-api/documents/#enumuploaddocumenttype
#[wire_enum]
pub enum DocumentType {
    /// `identity_verification`
    #[wire = "identity_verification"]
    IdentityVerification,
    /// `address_verification`
    #[wire = "address_verification"]
    AddressVerification,
    /// `date_of_birth_verification`
    #[wire = "date_of_birth_verification"]
    DateOfBirthVerification,
    /// `tax_id_verification`
    #[wire = "tax_id_verification"]
    TaxIdVerification,
    /// `account_approval_letter`
    #[wire = "account_approval_letter"]
    AccountApprovalLetter,
    /// `limited_trading_authorization`
    #[wire = "limited_trading_authorization"]
    LimitedTradingAuthorization,
    /// `w8ben`
    #[wire = "w8ben"]
    W8ben,
    /// `social_security_number_verification`
    #[wire = "social_security_number_verification"]
    SocialSecurityNumberVerification,
    /// The empty value.
    #[wire = ""]
    Null,
    /// `cip_result`
    #[wire = "cip_result"]
    CipResult,
    /// `other`
    #[wire = "other"]
    Other,
}

/// An enum representing the different fields to query for when listing accounts.
///
/// ie: asking for CONTACT and IDENTITY will have the api fill those fields when returning the list of Accounts however
/// other fields on the account will be nulled out where possible.
#[wire_enum]
pub enum AccountEntities {
    /// `contact`
    #[wire = "contact"]
    Contact,
    /// `identity`
    #[wire = "identity"]
    Identity,
    /// `disclosures`
    #[wire = "disclosures"]
    Disclosures,
    /// `agreements`
    #[wire = "agreements"]
    Agreements,
    /// `documents`
    #[wire = "documents"]
    Documents,
    /// `trusted_contact`
    #[wire = "trusted_contact"]
    TrustedContact,
    /// `trading_configurations`
    #[wire = "trading_configurations"]
    UserConfigurations,
}

/// An enum for representing what Clearing broker an Account is assigned to
#[wire_enum]
pub enum ClearingBroker {
    /// `APEX`
    #[wire = "APEX"]
    Apex,
    /// `ETC`
    #[wire = "ETC"]
    Etc,
    /// `IC`
    #[wire = "IC"]
    Ic,
    /// `VELOX`
    #[wire = "VELOX"]
    Velox,
    /// `VISION`
    #[wire = "VISION"]
    Vision,
    /// `SELF`
    #[wire = "SELF"]
    SelfClearing,
    /// `ALPACA_APCA`
    #[wire = "ALPACA_APCA"]
    AlpacaApca,
}

/// Enum representing what CIP provider was used.
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/accounts/accounts/#cip-provider>.
#[wire_enum]
pub enum CIPProvider {
    /// `alloy`
    #[wire = "alloy"]
    Alloy,
    /// `trulioo`
    #[wire = "trulioo"]
    Trulioo,
    /// `onfido`
    #[wire = "onfido"]
    Onfido,
    /// `veriff`
    #[wire = "veriff"]
    Veriff,
    /// `jumio`
    #[wire = "jumio"]
    Jumio,
    /// `getmati`
    #[wire = "getmati"]
    Getmati,
}

/// An enum representing the status of the `CIPInfo`
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/accounts/accounts/#cip-status>.
#[wire_enum(sorted)]
pub enum CIPStatus {
    /// `complete`
    #[wire = "complete"]
    Complete,
    /// `withdrawn`
    #[wire = "withdrawn"]
    Withdrawn,
}

/// See <https://docs.alpaca.markets/docs/api-references/broker-api/accounts/accounts/#cip-result>.
#[wire_enum(sorted)]
pub enum CIPResult {
    /// `clear`
    #[wire = "clear"]
    Clear,
    /// `consider`
    #[wire = "consider"]
    Consider,
}

/// Either `approved` or `rejected`
#[wire_enum(sorted)]
pub enum CIPApprovalStatus {
    /// `approved`
    #[wire = "approved"]
    Approved,
    /// `rejected`
    #[wire = "rejected"]
    Rejected,
}

/// Represents what kind information is inside a `TradeDocument`
///
/// Most likely will be either of these 3:
/// -  `ACCOUNT_STATEMENT`
/// -  `TRADE_CONFIRMATION`
/// -  `TAX_STATEMENT`
///
/// However, for older accounts with legacy documents the other legacy values might show up.
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/documents/#enumdocumenttype>.
#[wire_enum]
pub enum TradeDocumentType {
    /// `account_statement`
    #[wire = "account_statement"]
    AccountStatement,
    /// `trade_confirmation`
    #[wire = "trade_confirmation"]
    TradeConfirmation,
    /// `trade_confirmation_json`
    #[wire = "trade_confirmation_json"]
    TradeConfirmationJson,
    /// `tax_statement`
    #[wire = "tax_statement"]
    TaxStatement,
    /// `account_application`
    #[wire = "account_application"]
    AccountApplication,
    /// `tax_1099_b_details`
    #[wire = "tax_1099_b_details"]
    Tax1099BDetails,
    /// `tax_1099_b_form`
    #[wire = "tax_1099_b_form"]
    Tax1099BForm,
    /// `tax_1099_div_details`
    #[wire = "tax_1099_div_details"]
    Tax1099DivDetails,
    /// `tax_1099_div_form`
    #[wire = "tax_1099_div_form"]
    Tax1099DivForm,
    /// `tax_1099_int_details`
    #[wire = "tax_1099_int_details"]
    Tax1099IntDetails,
    /// `tax_1099_int_form`
    #[wire = "tax_1099_int_form"]
    Tax1099IntForm,
    /// `tax_w8`
    #[wire = "tax_w8"]
    TaxW8,
}

/// Represents additional information for whats inside a `TradeDocument` in combination with a `TradeDocumentType`
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/documents/#the-document-object>.
#[wire_enum]
pub enum TradeDocumentSubType {
    /// `1099-Comp`
    #[wire = "1099-Comp"]
    Type1099Comp,
    /// `1042-S`
    #[wire = "1042-S"]
    Type1042S,
    /// `480.6`
    #[wire = "480.6"]
    Type4806,
    /// `courtesy_statement`
    #[wire = "courtesy_statement"]
    CourtesyStatement,
}

/// Represents a sub type for an `UploadDocumentRequest`
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/documents/#enumuploaddocumentsubtype>.
#[wire_enum(sorted)]
pub enum UploadDocumentSubType {
    /// `Account Application`
    #[wire = "Account Application"]
    AccountApplication,
    /// `Form W-8BEN`
    #[wire = "Form W-8BEN"]
    FormW8Ben,
    /// `passport`
    #[wire = "passport"]
    Passport,
}

/// specifies the mime type of the base64 data you're uploading as part of a `UploadDocumentRequest`
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/documents/#parameters>.
#[wire_enum]
pub enum UploadDocumentMimeType {
    /// `application/pdf`
    #[wire = "application/pdf"]
    Pdf,
    /// `image/png`
    #[wire = "image/png"]
    Png,
    /// `image/jpeg`
    #[wire = "image/jpeg"]
    Jpeg,
    /// `application/json`
    #[wire = "application/json"]
    Json,
}

/// Represents the state that an `ACHRelationship` is in.
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/funding/ach/#attributes>.
#[wire_enum]
pub enum ACHRelationshipStatus {
    /// `QUEUED`
    #[wire = "QUEUED"]
    Queued,
    /// `APPROVED`
    #[wire = "APPROVED"]
    Approved,
    /// `PENDING`
    #[wire = "PENDING"]
    Pending,
}

/// Represents a kind of bank account.
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/funding/ach/#attributes>.
#[wire_enum]
pub enum BankAccountType {
    /// `CHECKING`
    #[wire = "CHECKING"]
    Checking,
    /// `SAVINGS`
    #[wire = "SAVINGS"]
    Savings,
    /// The empty value.
    #[wire = ""]
    None,
}

/// Represents a type of bank account.
///
/// Please see <https://docs.alpaca.markets/docs/api-references/broker-api/funding/bank/#creating-a-new-bank-relationship> for
/// more details.
#[wire_enum(sorted)]
pub enum IdentifierType {
    /// `ABA`
    #[wire = "ABA"]
    Aba,
    /// `BIC`
    #[wire = "BIC"]
    Bic,
}

/// Represents the states a Bank instance can be in.
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/funding/bank/#enumbankstatus>.
#[wire_enum]
pub enum BankStatus {
    /// `QUEUED`
    #[wire = "QUEUED"]
    Queued,
    /// `SENT_TO_CLEARING`
    #[wire = "SENT_TO_CLEARING"]
    SentToClearing,
    /// `APPROVED`
    #[wire = "APPROVED"]
    Approved,
    /// `CANCELED`
    #[wire = "CANCELED"]
    Canceled,
}

/// Represents the types of transfers that can be made.
///
/// Please see <https://docs.alpaca.markets/docs/api-references/broker-api/funding/transfers/#enumtransfertype> for more
/// details.
#[wire_enum(sorted)]
pub enum TransferType {
    /// `ach`
    #[wire = "ach"]
    Ach,
    /// `wire`
    #[wire = "wire"]
    Wire,
}

/// Represents the states a Transfer instance can be in.
///
/// Please see <https://docs.alpaca.markets/docs/api-references/broker-api/funding/transfers/#enumtransferstatus> for more
/// details.
#[wire_enum]
pub enum TransferStatus {
    /// `QUEUED`
    #[wire = "QUEUED"]
    Queued,
    /// `APPROVAL_PENDING`
    #[wire = "APPROVAL_PENDING"]
    ApprovalPending,
    /// `PENDING`
    #[wire = "PENDING"]
    Pending,
    /// `SENT_TO_CLEARING`
    #[wire = "SENT_TO_CLEARING"]
    SentToClearing,
    /// `REJECTED`
    #[wire = "REJECTED"]
    Rejected,
    /// `CANCELED`
    #[wire = "CANCELED"]
    Canceled,
    /// `APPROVED`
    #[wire = "APPROVED"]
    Approved,
    /// `SETTLED`
    #[wire = "SETTLED"]
    Settled,
    /// `COMPLETE`
    #[wire = "COMPLETE"]
    Complete,
    /// `RETURNED`
    #[wire = "RETURNED"]
    Returned,
}

/// Represents the direction of the transfer.
///
/// Please see <https://docs.alpaca.markets/docs/api-references/broker-api/funding/transfers/#enumtransferdirection> for more
/// details.
#[wire_enum(sorted)]
pub enum TransferDirection {
    /// `INCOMING`
    #[wire = "INCOMING"]
    Incoming,
    /// `OUTGOING`
    #[wire = "OUTGOING"]
    Outgoing,
}

/// Represents the timing of a transfer.
///
/// Please see <https://docs.alpaca.markets/docs/api-references/broker-api/funding/transfers/#creating-a-transfer-entity> for
/// more details.
#[wire_enum(sorted)]
pub enum TransferTiming {
    /// `immediate`
    #[wire = "immediate"]
    Immediate,
}

/// Represents who is responsible for paying fees associated with the transfer.
///
/// Please see <https://docs.alpaca.markets/docs/api-references/broker-api/funding/transfers/#enumfeepaymentmethod> for more
/// details.
#[wire_enum]
pub enum FeePaymentMethod {
    /// `user`
    #[wire = "user"]
    User,
    /// `invoice`
    #[wire = "invoice"]
    Invoice,
}

/// Represents the types of journals. Cash journals are transfers of cash.
/// Security journals are transfers of securities like stocks.
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/journals/>.
#[wire_enum(sorted)]
pub enum JournalEntryType {
    /// `JNLC`
    #[wire = "JNLC"]
    Cash,
    /// `JNLS`
    #[wire = "JNLS"]
    Security,
}

/// The various states a journal can be in during its lifecycle.
///
/// See <https://docs.alpaca.markets/docs/api-references/broker-api/journals/#enumjournalstatus>.
#[wire_enum]
pub enum JournalStatus {
    /// `queued`
    #[wire = "queued"]
    Queued,
    /// `sent_to_clearing`
    #[wire = "sent_to_clearing"]
    SentToClearing,
    /// `pending`
    #[wire = "pending"]
    Pending,
    /// `executed`
    #[wire = "executed"]
    Executed,
    /// A non-trade activity has been created for the journal.
    ///
    /// Cash journals on the v2 API only.
    #[wire = "activity_created"]
    ActivityCreated,
    /// `rejected`
    #[wire = "rejected"]
    Rejected,
    /// `canceled`
    #[wire = "canceled"]
    Canceled,
    /// `refused`
    #[wire = "refused"]
    Refused,
    /// `correct`
    #[wire = "correct"]
    Correct,
    /// `deleted`
    #[wire = "deleted"]
    Deleted,
}

/// The possible values of the Portfolio status.
///
/// See <https://docs.alpaca.markets/reference/get-v1-rebalancing-portfolios>.
#[wire_enum(sorted)]
pub enum PortfolioStatus {
    /// `active`
    #[wire = "active"]
    Active,
    /// `inactive`
    #[wire = "inactive"]
    Inactive,
    /// `needs_adjustment`
    #[wire = "needs_adjustment"]
    NeedsAdjustment,
}

/// The possible values of the Weight type.
///
/// See <https://docs.alpaca.markets/reference/post-v1-rebalancing-portfolios>.
#[wire_enum]
pub enum WeightType {
    /// `cash`
    #[wire = "cash"]
    Cash,
    /// `asset`
    #[wire = "asset"]
    Asset,
}

/// The possible values of the Rebalancing Conditions type.
///
/// See <https://docs.alpaca.markets/reference/post-v1-rebalancing-portfolios>.
#[wire_enum]
pub enum RebalancingConditionsType {
    /// `drift_band`
    #[wire = "drift_band"]
    DriftBand,
    /// `calendar`
    #[wire = "calendar"]
    Calendar,
}

/// The possible values of the Rebalancing Conditions subtype for `drift_band`.
///
/// See <https://docs.alpaca.markets/reference/post-v1-rebalancing-portfolios>.
#[wire_enum(sorted)]
pub enum DriftBandSubType {
    /// `absolute`
    #[wire = "absolute"]
    Absolute,
    /// `relative`
    #[wire = "relative"]
    Relative,
}

/// The possible values of the Rebalancing Conditions subtype for `drift_band`.
///
/// See <https://docs.alpaca.markets/reference/post-v1-rebalancing-portfolios>.
#[wire_enum]
pub enum CalendarSubType {
    /// `weekly`
    #[wire = "weekly"]
    Weekly,
    /// `monthly`
    #[wire = "monthly"]
    Monthly,
    /// `quarterly`
    #[wire = "quarterly"]
    Quarterly,
    /// `annually`
    #[wire = "annually"]
    Annually,
}

/// The possible values of the Run type.
///
/// See <https://docs.alpaca.markets/reference/post-v1-rebalancing-runs>.
#[wire_enum(sorted)]
pub enum RunType {
    /// `full_rebalance`
    #[wire = "full_rebalance"]
    FullRebalance,
    /// `invest_cash`
    #[wire = "invest_cash"]
    InvestCash,
}

/// The possible values of the `initiated_from` field.
///
/// See <https://docs.alpaca.markets/docs/portfolio-rebalancing>.
#[wire_enum]
pub enum RunInitiatedFrom {
    /// `system`
    #[wire = "system"]
    System,
    /// `api`
    #[wire = "api"]
    Api,
}

/// The possible values of the Run status.
///
/// See <https://docs.alpaca.markets/reference/get-v1-rebalancing-runs>.
#[wire_enum]
pub enum RunStatus {
    /// `QUEUED`
    #[wire = "QUEUED"]
    Queued,
    /// `IN_PROGRESS`
    #[wire = "IN_PROGRESS"]
    InProgress,
    /// `CANCELED`
    #[wire = "CANCELED"]
    Canceled,
    /// `CANCELED_MID_RUN`
    #[wire = "CANCELED_MID_RUN"]
    CanceledMidRun,
    /// `ERROR`
    #[wire = "ERROR"]
    Error,
    /// `TIMEOUT`
    #[wire = "TIMEOUT"]
    Timeout,
    /// `COMPLETED_SUCCESS`
    #[wire = "COMPLETED_SUCCESS"]
    CompletedSuccess,
    /// `COMPLETED_ADJUSTED`
    #[wire = "COMPLETED_ADJUSTED"]
    CompletedAdjusted,
}
