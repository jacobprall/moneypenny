import { combineReducers } from 'redux'

import accountModalReducer from './account_modal_reducer'

export default combineReducers({
  account: accountModalReducer
})