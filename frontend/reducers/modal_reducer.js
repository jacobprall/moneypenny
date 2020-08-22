import { combineReducers } from 'redux'

import accountModalReducer from './modal_reducers/account_modal_reducer'
import transactionModalReducer from './modal_reducers/transaction_modal_reducer'
import componentModalReducer from './modal_reducers/component_modal_reducer'
import formTypeModalReducer from './modal_reducers/formType_modal_reducer'


export default combineReducers({
  account: accountModalReducer, 
  transaction: transactionModalReducer,
  component: componentModalReducer,
  formType: formTypeModalReducer
})