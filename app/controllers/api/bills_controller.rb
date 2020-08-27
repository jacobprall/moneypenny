class Api::BillsController < ApplicationController
  
  
  def index
    @bills = current_user.bills 
    render :index
  end

  def create
    @bill = Bill.create(bill_params)
    if @bill.save
      render 'api/bills/update'
    else
      render json: @bill.errors.full_messages, status: 422
    end
  end

  def update
    @bill = Bill.find(params[:id])
    
    if @bill.update(bill_params)
      render 'api/bills/update'
    else
      render json: @bill.errors.full_messages, status: 422
    end
  end

  def destroy
    @bill = current_user.bills.find(params[:id])
    if @bill.recurring
      @bill.due_date = @bill.due_date.next_month
      @bill.save
      render json: @bill
    else
      @bill.destroy 
      render json: @bill
    end
    
  end

  def bill_params
    params.require(:bill).permit(:amount, :due_date, :name, :recurring, :user_id)
  end

end
