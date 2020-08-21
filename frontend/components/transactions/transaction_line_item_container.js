import {
  connect
} from 'react-redux'
import {deleteTransaction} from '../../actions/transaction_actions'
import TransactionLineItem from './transaction_line_item'


const mSTP = (state, ownProps) => ({
  transaction: ownProps.transaction
})

const mDTP = (dispatch) => ({
  deleteTransaction: (transactionId) => dispatch(deleteTransaction(transactionId))
})

export default connect(mSTP, mDTP)(TransactionLineItem)
