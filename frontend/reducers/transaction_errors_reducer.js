import { RECEIVE_TRANSACTION_ERRORS, CLEAR_TRANSACTION_ERRORS } from '../actions/transaction_actions'

const transactionErrorsReducer = (oldState = [], action) => {
  let newState = [];
  switch (action.type) {
    case RECEIVE_TRANSACTION_ERRORS:
      if (action.errors !== undefined) return action.errors;
      return newState
    case CLEAR_TRANSACTION_ERRORS:
      return newState;
    default:
      return oldState;
  }
}
export default transactionErrorsReducer