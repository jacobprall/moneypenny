const {
  RECEIVE_ACCOUNT_ERRORS,
  CLEAR_ACCOUNT_ERRORS,
  RECEIVE_ACCOUNT
} = require("../actions/account_actions");

const accountErrorsReducer = (oldState = [], action) => {
  let newState = Object.assign({}, oldState);
  switch (action.type) {
    case RECEIVE_ACCOUNT_ERRORS:
      return action.errors;
    case RECEIVE_ACCOUNT:
      newState.errors = [];
      return newState;
    default:
      return oldState;
  }
}

export default accountErrorsReducer