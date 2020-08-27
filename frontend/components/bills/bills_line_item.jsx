import React from 'react'
import BillsFormContainer from './bills_form_container'
import { useSelector, useDispatch } from 'react-redux'
import commaFormat from '../../util/number_formatter'
import { deleteBill, updateBill } from '../../actions/bill_actions'
import { openModal } from '../../actions/modal_actions'
import { formatDate } from '../../util/date_util'

export default function bills_line_item({bill}) {

  const dispatch = useDispatch()
  let { id, name, due_date, user_id, recurring, amount } = bill

  const modalOpener = (formType, component, payload) => dispatch(openModal(formType, component, payload))
  
  const billDeleter = (billId) => {
    if (bill.recurring === false) {
      dispatch(updateBill(bill)).then(
        dispatch(deleteBill(billId))
      )} else {
        dispatch(deleteBill(billId))
      }

    }
   
  const month = formatDate(due_date).split(' ')[0]
  const day = formatDate(due_date).split(' ')[1].split(',')[0]
  // if (!amount)

  return (
    <li className="bill" >
      <div className="bill-left">
        <div className="bill-due-date">{month} <br /> {day}</div>
        <div className="bill-info">{name}</div>
      </div>
      <div className="bill-right">
        <span onClick={() => billDeleter(bill.id)}>Mark as Paid</span>
        <span className="edit-bill" onClick={() => modalOpener('edit', BillsFormContainer, bill)}>Edit Bill</span>
        <span>${commaFormat(amount.toFixed(2).toString())}</span>
      </div>
    </li>
  );
}
