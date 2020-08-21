import React from 'react'
import { formatDate } from '../../util/date_util'
import { openModal } from '../../actions/modal_actions'
export default function transaction_line_item({transaction, deleteTransaction, commaFormat}) {
    const { date, description, amount, transaction_category, id } = transaction //destructuring
    

    return (
      <tr key={id} onClick={(e) => openModal('edit transaction', e.currentTarget.value)} className="table-row" value={transaction}>
        <td className="table-row-data">{`${commaFormat((amount.toFixed(2).toString()))}`}</td>
        <td className="table-row-data">{formatDate(date)}</td>
        <td className="table-row-data">{description}</td>
        <td className="table-row-data">{transaction_category}</td>
        <td className="delete-transaction" onClick={deleteTransaction}>X</td>
      </tr>
    )

}
