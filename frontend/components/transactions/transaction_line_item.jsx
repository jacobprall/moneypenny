import React from 'react'
import { formatDate } from '../../util/date_util'
export default function transaction_line_item({transaction, deleteTransaction}) {
    const { date, description, amount, transaction_category, id } = transaction //destructuring
    
    const handleClick = (e) => {
      const id = e.currentTarget.value.id
      //conditionally render a drop down to edit?
    }

    return (
      <tr key={id} onClick={handleClick} className="table-row">
        <td className="table-row-data">{amount.toFixed(2)}</td>
        <td className="table-row-data">{formatDate(date)}</td>
        <td className="table-row-data">{description}</td>
        <td className="table-row-data">{transaction_category}</td>
        <td className="delete-transaction" onClick={deleteTransaction}>X</td>
      </tr>
    )

}
