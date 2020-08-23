import React from 'react'
import { formatDate } from '../../util/date_util'
import { openModal } from '../../actions/modal_actions'
import TransactionFormContainer from './transaction_form/transaction_form_container'
import commaFormat from '../../util/number_formatter'
import { useDispatch } from 'react-redux'
import  {deleteTransaction} from '../../actions/transaction_actions'

export default function transaction_line_item({ transaction }) {
  const dispatch = useDispatch();
  const modalOpener = (formType, component, payload) => dispatch(openModal(formType, component, payload))
  const transactionDeleter = (transactionId) => dispatch(deleteTransaction(transactionId))
  const { date, description, amount, transaction_category, id } = transaction;
  

  return (
    <tr key={id} onClick={(e) => modalOpener('edit', TransactionFormContainer, transaction)} className="table-row" value={transaction}>
      <td className="table-row-data">{`${commaFormat((amount.toFixed(2).toString()))}`}</td>
      <td className="table-row-data">{formatDate(date)}</td>
      <td className="table-row-data">{description}</td>
      <td className="table-row-data">{transaction_category}</td>
      <td className="delete-transaction" onClick={() => transactionDeleter(id)}>X</td>
    </tr>
  )

}
