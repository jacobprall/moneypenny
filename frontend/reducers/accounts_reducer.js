import { RECEIVE_ACCOUNTS, RECEIVE_ACCOUNT, REMOVE_ACCOUNT} from '../actions/account_actions'
import { LOGOUT_CURRENT_USER } from '../actions/session_actions';

const accountsReducer = (oldState = {}, action) => {
  let newState = Object.assign(oldState, {})
  
  switch (action.type) {
    case RECEIVE_ACCOUNTS:
      newState = action.accounts
      return newState;
    case RECEIVE_ACCOUNT:
      let newAccount = {[action.account.id]: action.account}
      newState = Object.assign(newState, newAccount);
      return newState;
    case REMOVE_ACCOUNT:
      delete newState[action.accountId];
      console.log(newState)
      return newState;
    case LOGOUT_CURRENT_USER:
      return {}
    default:
      return newState;
  }
}

export default accountsReducer