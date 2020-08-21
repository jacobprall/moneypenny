import { connect } from 'react-redux'
import TransactionIndex from './transaction_index'
import { requestTransactions, createTransaction} from '../../actions/transaction_actions'
const mSTP = ({entities: {transactions}}) => ({
  transactions: Object.values(transactions)
})

const mDTP = (dispatch) => ({
  requestTransactions: () => dispatch(requestTransactions()),
  createTransaction: (transaction) => dispatch(createTransaction(transaction))
})

export default connect(mSTP, mDTP)(TransactionIndex)
