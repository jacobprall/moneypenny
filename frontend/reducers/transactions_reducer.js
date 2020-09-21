import { RECEIVE_TRANSACTIONS, RECEIVE_TRANSACTION, REMOVE_TRANSACTION, RECEIVE_TRANSACTION_SEARCH, CLEAR_TRANSACTION_SEARCH } from '../actions/transaction_actions'


const transactionsReducer = (oldState = {}, action) => {
  let newState = Object.assign({}, oldState);

  switch (action.type) {
    case RECEIVE_TRANSACTIONS:
      newState = action.transactions;
      return newState;
    case RECEIVE_TRANSACTION: 
      let newTransaction = {[action.transaction.id]: action.transaction};
      newState = Object.assign(newState, newTransaction);
      return newState;
    case REMOVE_TRANSACTION:
      delete newState[action.transactionId];
      return newState;
    case RECEIVE_TRANSACTION_SEARCH:
      return action.transactions;
    case CLEAR_TRANSACTION_SEARCH:
      newState = action.transactions;
      return newState;
    default:
      return newState;
  }
}

export default transactionsReducer