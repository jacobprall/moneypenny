import {
  connect
} from 'react-redux'
import Overview from './overview'
import {
  requestAccounts
} from '../../actions/account_actions'

const mSTP = (state) => ({

})

const mDTP = dispatch => ({
  getAccounts: () => (dispatch(requestAccounts()))
})

export default connect(mSTP, mDTP)(Overview)