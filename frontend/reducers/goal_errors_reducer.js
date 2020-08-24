import { RECEIVE_GOAL_ERRORS, CLEAR_GOAL_ERRORS } from '../actions/goal_actions'

const goalErrorsReducer = (oldState = [], action) => {
  let newState = [];
  switch (action.type) {
    case RECEIVE_GOAL_ERRORS:
      if (action.errors !== undefined) return action.errors;
      return newState;
    case CLEAR_GOAL_ERRORS:
      return newState;
    default:
      return oldState;
  }
}

export default goalErrorsReducer