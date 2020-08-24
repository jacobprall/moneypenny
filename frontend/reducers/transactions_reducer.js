import { RECEIVE_TRANSACTIONS, RECEIVE_TRANSACTION, REMOVE_TRANSACTION } from '../actions/transaction_actions'


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
    default:
      return newState;
  }
}

export default transactionsReducer