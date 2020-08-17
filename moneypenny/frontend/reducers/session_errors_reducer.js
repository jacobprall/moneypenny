const { RECEIVE_SESSION_ERRORS, RECEIVE_CURRENT_USER } = require("../actions/session_actions");

const sessionErrorsReducer = (oldState = [], action) => {
  let newState = Object.assign({}, oldState);
  switch (action.type) {
    case RECEIVE_SESSION_ERRORS:
      newState.session = action.errors;
      return newState;
    case RECEIVE_CURRENT_USER:
      newState.errors = null;
      return newState;
    default:
      return oldState;
  }
}

export default sessionErrorsReducer