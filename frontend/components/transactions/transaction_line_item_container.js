import {
  connect
} from 'react-redux'
import {deleteTransaction} from '../../actions/transaction_actions'
import TransactionLineItem from './transaction_line_item'
import commaFormat from '../../util/number_formatter'
import { openModal } from '../../actions/modal_actions'

const mSTP = (state, ownProps) => ({
  transaction: ownProps.transaction, 
  commaFormat: commaFormat
})

const mDTP = (dispatch) => ({
  deleteTransaction: (transactionId) => dispatch(deleteTransaction(transactionId)),
  openModal: (modalType, transaction) => dispatch(openModal(modalType, transaction))
})

export default connect(mSTP, mDTP)(TransactionLineItem)
