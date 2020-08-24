import { combineReducers } from 'redux'
import userReducer from './user_reducer'
import accountsReducer from './accounts_reducer'
import transactionsReducer from './transactions_reducer'
import goalsReducer from './goals_reducer'

const entitiesReducer = combineReducers({ 
  users: userReducer, 
  accounts: accountsReducer,
  transactions: transactionsReducer,
  goals: goalsReducer
})

export default entitiesReducer