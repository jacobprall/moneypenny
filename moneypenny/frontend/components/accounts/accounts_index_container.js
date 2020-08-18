import { connect } from 'react-redux'
import AccountsIndex from './accounts_index'
import allAccounts from '../../reducers/selector'

const mSTP = state => ({
  accounts: allAccounts(state)
})

const mDTP = dispatch => ({

})

export default connect(mSTP, mDTP)(AccountsIndex)