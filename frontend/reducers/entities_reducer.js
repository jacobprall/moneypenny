import { combineReducers } from 'redux'
import userReducer from './user_reducer'
import accountsReducer from './accounts_reducer'

const entitiesReducer = combineReducers({ 
  users: userReducer, 
  accounts: accountsReducer
})

export default entitiesReducer