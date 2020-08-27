import { RECEIVE_BILL_ERRORS, CLEAR_BILL_ERRORS } from '../actions/bill_actions'

const billErrorsReducer = (oldState = [], action) => {
  let newState = [];
  switch (action.type) {
    case RECEIVE_BILL_ERRORS:
      if (action.errors !== undefined) return action.errors;
      return newState;
    case CLEAR_BILL_ERRORS:
      return newState;
    default:
      return oldState;
  }
}

export default billErrorsReducer