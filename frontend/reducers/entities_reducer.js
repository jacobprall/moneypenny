import { combineReducers } from 'redux'
import userReducer from './user_reducer'
import accountsReducer from './accounts_reducer'
import transactionsReducer from './transactions_reducer'
import goalsReducer from './goals_reducer'
import billsReducer from './bills_reducer'
import newsReducer from './news_reducer'

const entitiesReducer = combineReducers({ 
  users: userReducer, 
  accounts: accountsReducer,
  transactions: transactionsReducer,
  goals: goalsReducer,
  bills: billsReducer,
  news: newsReducer
})

export default entitiesReducer