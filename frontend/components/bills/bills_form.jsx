import React, { useState } from 'react'
import { useDispatch } from 'react-redux'
import { updateBill } from '../../actions/bill_actions'
export default function bills_form({ props: {selectedData, billDeleter, processForm, modalCloser, billErrorsClearer }}) {
  const { errors, formType, passedBill } = selectedData;
  const dispatch = useDispatch()
  const [bill, setBill] = useState(passedBill);
  const update = (field) => (e) =>
      setBill({ ...bill, [field]: e.currentTarget.value });
  
  const handleSubmit = (e) => {
    e.preventDefault();
    console.log(bill)
    processForm(bill)
      .then(() => modalCloser())
      .then(() => billErrorsClearer());
  };
  const handleClose = (e) => {
    e.preventDefault();
    modalCloser();
    billErrorsClearer();
  };

  const handleToggle = (e) => {
    if (bill.recurring) {
      setBill({ ...bill, recurring: false}) 
    } else {
      setBill({ ...bill, recurring: true })
      };
  }

  const renderErrors = () => (
    <ul className="modal-form-errors">
      {errors.map((error, i) => (
       <li className="modal-form-error" key={i}>{error}</li>
        ) 
      )}
    </ul>
  );

    const deleteOption = () => {
      if (formType === 'edit') {
        return (
          <span className='edit-delete' onClick={() => billDeleter(bill)}>Delete Bill</span>
        )
      }
    };


  return (
    <form onSubmit={handleSubmit} className="modal-form">
      <div onClick={handleClose} className="close-x">X</div>
      <div className="modal-inputs">
        <label>Name:
          <input type="text" value={bill.name} onChange={update('name')}/>
        </label>
        <label>Amount:
          <input type="number" value={bill.amount} onChange={update('amount')}/>
        </label>
        <label>Due Date:
          <input type="datetime-local" value={bill.due_date} onChange={update('due_date')}/>
        </label>
        <label>Recurring:
          <input type="checkbox" value={!bill.recurring}  onChange={handleToggle} checked={bill.recurring}/>
        </label>
        {/* <input type="hidden" value={user_id} name="user_id" /> */}
        <button className="modal-form-submit" value={formType}>{formType.toUpperCase()} BILL</button>
          {deleteOption()}
          {renderErrors()}
      </div>
      
    </form>
  )
}
