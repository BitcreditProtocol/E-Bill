# 0.3.8

* Add Blank Endorse Bill data model implementation
    * Rename `IdentityPublicData` to `BillIdentifiedParticipant`
        * same for `LightIdentityPublicData`
    * Introduce the concept of `BillParticipant`, with the variants `Identified` and `Anonymous`
        * `Anonymous` includes a `BillAnonymousParticipant`
        * `Identified` includes a `BillIdentifiedParticipant`
    * Use `BillParticipant` in parts of the bill where a participant can be anonymous

# 0.3.7

* Fix request recourse to accept validation - does not require a request to accept anymore

# 0.3.6

* Add validation for maturity date
* Add docs for testing
* Fix reject to accept not showing correctly without req to accept
* Add endpoint `clear_bill_cache` to clear the bill cache

# 0.3.5

* Properly propagate and log errors when getting a file (e.g. an avatar)
* Several fixes to recourse bill action validation
* Add in-depth tests for bill validation
* Fix not checking contact for company files

# 0.3.4

* Add in-depth tests for bill validation
* Add recourse reason to `Recourse` block data
    * (breaks existing persisted bills, if they had a recourse block)
* Added `has_requested_funds` flag to `BillStatusWeb`, indicating the caller has requested funds (req to pay, req to recourse, offer to sell) at some point
* Added `past_payments` endpoint to `Api.bill()`, which returns data about past payments and payment requests where the caller was the beneficiary

# 0.3.3

* Use Nip-04 as a default for Nostr communication
* Add incoming bill validation
* Add block data validation
* Add bill action validation for incoming blocks
* Add signer verification for incoming blocks
* Add recourse reason to `RequestRecourse` block data
    * (breaks existing persisted bills, if they had a request recourse block)
* Move bill validation logic to `bcr-ebill-core`

# 0.3.2

* Fixed `request_to_accept` calling the correct action
* Multi-identity Nostr consumer and currently-active-identity-sending
* Added more thorough logging, especially debug logging
* Expose Error types to TS
* Use string for `log_level` in config

# 0.3.1

* Persist active Identity to DB for WASM
* Change indexed-db name to "data"
* Use a different indexed-db collection for files, named "files"
* Create a new indexeddb database connection for each query to avoid transaction overlapping
* Removed timezone db api
* Persist base64 string instead of bytes for images, for more efficiency
* Added Retry-sending for Nostr block events
* Added block propagation via Nostr
* Added a caching layer for bills, heavily improving performance
* Added `error` logs for all errors returned from the API for the WASM version
* Added `log_level` to Config, which defaults to `info`
* Changed the API for uploading files to bill to use `file` instead of `files`.
So files can only be uploaded individually, but for `issue()`, `file_upload_ids`
can be passed - a list of file upload ids, to upload multiple files for one bill.
* Restructured `BitcreditBillWeb` to a more structured approach, separating `status`,
`data` and `participants` and adding the concept of `current_waiting_state`, to
have all data available, if the bill is in a waiting state.
    * Added the concept of `redeemed_funds_available` on `status`, to indicate if
    the caller has funds available (e.g. from a sale, or a paid bill)

# 0.3.0

* First version exposing a WASM API
