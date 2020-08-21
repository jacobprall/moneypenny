import { connect } from 'react-redux'
import AccountsIndex from './accounts_index'
import allAccounts from '../../reducers/selector'
import { requestAccounts } from '../../actions/account_actions'
import commaFormat from '../../util/number_formatter'
const mSTP = ({entities: {accounts}}) => ({
  accounts: Object.values(accounts)
})

const mDTP = dispatch => ({
  getAccounts: () => (dispatch(requestAccounts())),
  commaFormat: (amt) => commaFormat(amt)

})

export default connect(mSTP, mDTP)(AccountsIndex)