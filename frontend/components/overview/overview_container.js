import {
  connect
} from 'react-redux'
import Overview from './overview'
import {
  requestAccounts
} from '../../actions/account_actions'
import { requestTransactions } from '../../actions/transaction_actions'

const mSTP = (state) => ({

})

const mDTP = dispatch => ({
  getAccounts: () => (dispatch(requestAccounts())),
  getTransactions: () => (dispatch(requestTransactions()))
})

export default connect(mSTP, mDTP)(Overview)