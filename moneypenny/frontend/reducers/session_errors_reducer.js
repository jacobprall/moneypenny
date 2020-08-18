const { RECEIVE_SESSION_ERRORS, RECEIVE_CURRENT_USER, CLEAR_SESSION_ERRORS } = require("../actions/session_actions");

const sessionErrorsReducer = (oldState = [], action) => {
  let newState = Object.assign({}, oldState);
  switch (action.type) {
    case RECEIVE_SESSION_ERRORS:
      return action.errors;
    case CLEAR_SESSION_ERRORS:
      newState.errors = [];
      return newState;
    case RECEIVE_CURRENT_USER:
      newState.errors = [];
      return newState;
    default:
      return oldState;
  }
}

export default sessionErrorsReducer