import React from 'react'
import TransactionLineItemContainer from './transaction_line_item_container'

export default function transaction_index({transactions, createTransaction}) {
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
       <button className="add-transaction" onClick={createTransaction}>+ Add Transaction</button>
       <input type="text"/>
        </div>
        
      <table>
          {renderTableHeader()}
          <th className="delete-column"><img src={window.trashCan} className="trash-can" /></th>
        {/* <div className="table-headers"> */}
          
        {/* </div> */}
        {/* <div className="table-rows-container"> */}
          {renderTransactions()}
        {/* </div> */}
      </table>
      </div>
      
    </div>
  )
}
