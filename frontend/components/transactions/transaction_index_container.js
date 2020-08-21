import { connect } from 'react-redux'
import TransactionIndex from './transaction_index'
import { requestTransactions, createTransaction} from '../../actions/transaction_actions'
import { openModal } from '../../actions/modal_actions'

const mSTP = ({entities: {transactions}}) => ({
  transactions: Object.values(transactions)
})

const mDTP = (dispatch) => ({
  requestTransactions: () => dispatch(requestTransactions()),
  openModal: modalType => dispatch(openModal(modalType))
})

export default connect(mSTP, mDTP)(TransactionIndex)
