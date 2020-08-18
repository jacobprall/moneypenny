import { connect } from 'react-redux'
import AccountsIndex from './accounts_index'
import allAccounts from '../../reducers/selector'
import { requestAccounts } from '../../actions/account_actions'

const mSTP = ({entities: {accounts}}) => ({
  accounts
})

const mDTP = dispatch => ({
  getAccounts: () => (dispatch(requestAccounts()))
})

export default connect(mSTP, mDTP)(AccountsIndex)