//! Explicit window action for independent request authorization.

use std::path::Path;

use adroit_model::RequestId;

use super::store::{ControlError, IndependentAuthorization, IndependentAuthorizationPlan, Store};

pub fn authorize(
    root: &Path,
    request: RequestId,
    plan: IndependentAuthorizationPlan,
) -> Result<IndependentAuthorization, ControlError> {
    let mut store = Store::open(&root.join("state.sqlite3"))?;
    store
        .authorize_independent_run(request, plan)
        .map(|reply| reply.value)
}
