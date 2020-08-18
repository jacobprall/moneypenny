const { RECEIVE_CURRENT_USER, LOGOUT_CURRENT_USER } = require("../actions/session_actions");


const sessionReducer = (oldState = { id: null }, action) => {
  let nextState = Object.assign({}, oldState);
  switch (action.type) {
    case RECEIVE_CURRENT_USER:
      nextState.id = action.user.id;
      return nextState;
    case LOGOUT_CURRENT_USER:
      nextState.id = null;
      return nextState;
    default:
      return oldState;
  }
}

export default sessionReducer


