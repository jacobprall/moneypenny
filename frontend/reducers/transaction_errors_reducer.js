import { RECEIVE_TRANSACTION_ERRORS, CLEAR_TRANSACTION_ERRORS } from '../actions/transaction_actions'
import transactionsReducer from './transactions_reducer';

const transactionErrorsReducer = (oldState = [], action) => {
  let newState = [];
  switch (action.type) {
    case RECEIVE_TRANSACTION_ERRORS:
      if (action.errors.length) return action.errors;
      return newState;
    case CLEAR_TRANSACTION_ERRORS:
      return newState;
    default:
      return oldState;
  }
}
export default transactionErrorsReducer