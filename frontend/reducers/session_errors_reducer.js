const { RECEIVE_SESSION_ERRORS, RECEIVE_CURRENT_USER, CLEAR_SESSION_ERRORS, LOGOUT_CURRENT_USER } = require("../actions/session_actions");

const sessionErrorsReducer = (oldState = [], action) => {
  let newState = [].concat(oldState);
  switch (action.type) {
    case RECEIVE_SESSION_ERRORS:
      return action.errors;
    case CLEAR_SESSION_ERRORS:
      newState.errors = [];
      return newState;
    case RECEIVE_CURRENT_USER:
      newState.errors = [];
      return newState;
    case LOGOUT_CURRENT_USER:
      newState.errors = [];
    default:
      return oldState;
  }
}

export default sessionErrorsReducer