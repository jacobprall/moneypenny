const {
  RECEIVE_ACCOUNT_ERRORS,
  CLEAR_ACCOUNT_ERRORS,
  RECEIVE_ACCOUNT
} = require("../actions/account_actions");

const accountErrorsReducer = (oldState = [], action) => {
  let newState = [];
  switch (action.type) {
    case RECEIVE_ACCOUNT_ERRORS:
      if (action.errors !== undefined) return action.errors;
      return newState
    case CLEAR_ACCOUNT_ERRORS:
      newState = [];
      return newState;
    default:
      return oldState;
  }
}

export default accountErrorsReducer