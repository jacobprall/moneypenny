import { combineReducers } from 'redux'

import accountModalReducer from './account_modal_reducer'
import transactionModalReducer from './transaction_modal_reducer'

export default combineReducers({
  account: accountModalReducer, 
  transaction: transactionModalReducer
})