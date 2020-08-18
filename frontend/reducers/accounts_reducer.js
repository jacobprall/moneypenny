import { RECEIVE_ACCOUNTS, RECEIVE_ACCOUNT, REMOVE_ACCOUNT} from '../actions/account_actions'

const accountsReducer = (oldState = [], action) => {
  let newState = [].concat(oldState);
  
  switch (action.type) {
    case RECEIVE_ACCOUNTS:
      newState = action.accounts
      return newState;
    case RECEIVE_ACCOUNT:
      let newAccount = { [action.account.id]: action.account }
      newState = Object.assign(newState, newAccount);
      return newState;
    case REMOVE_ACCOUNT:
      delete newState[action.accountId];
      return newState;
    default:
      return oldState;
  }
}

export default accountsReducer