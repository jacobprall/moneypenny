import { connect } from 'react-redux'
import AccountsIndex from './accounts_index'
import allAccounts from '../../reducers/selector'
import { requestAccounts } from '../../actions/account_actions'
import {openModal} from '../../actions/modal_actions'
const mSTP = ({entities: {accounts}}) => ({
  accounts: Object.values(accounts)
})

const mDTP = dispatch => ({
  getAccounts: () => (dispatch(requestAccounts())),
})

export default connect(mSTP, mDTP)(AccountsIndex)