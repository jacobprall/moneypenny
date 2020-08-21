import React from 'react'
import TransactionLineItemContainer from './transaction_line_item_container'

export default function transaction_index({transactions, openModal}) {
  const renderTransactions = () => (
    transactions.map((transaction) => (
      <TransactionLineItemContainer transaction={transaction} />
    ))
  );

  function renderTableHeader() {
    if (transactions.length) {
    let header = Object.keys(transactions[0])
    return header.map((key, index) => {
      if (key !== 'id' && key !== 'tags' && key !== 'account_id') {
        return <th key={index}>{key.toUpperCase()}</th>
      }
    })
    }
  }

  return (
    <div className="transactions-index-container">
      <div className="transactions">
        <div className="above-table">
          <button className="add-transaction" onClick={() => openModal('new transaction')}>+ Add Transaction</button>
          <input type="text"/>
        </div>
        <table>
          <thead>
            <tr>
              {renderTableHeader()}
              <th className="delete-column"><img src={window.trashCan} className="trash-can" /></th>
            </tr>
            </thead>
          <tbody>
            {renderTransactions()}
          </tbody>
        </table>
      </div>
    </div>
  )
}
