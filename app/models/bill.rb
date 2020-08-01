# == Schema Information
#
# Table name: bills
#
#  id         :bigint           not null, primary key
#  amount_due :decimal(8, 2)    not null
#  details    :string
#  due_date   :datetime
#  name       :string           not null
#  paid       :boolean          not null
#  recurring  :integer          not null
#  created_at :datetime         not null
#  updated_at :datetime         not null
#  user_id    :integer
#
# Indexes
#
#  index_bills_on_user_id  (user_id)
#
class Bill < ApplicationRecord
  validates :amount_due, :name, :paid, :recurring, :user_id, presence: true

  belongs_to :user,
  foreign_key: :user_id,
  class_name: :User 

  def recurring_freq
    if self.recurring == 0
      @recurrance = "None"
    elsif self.recurring == 1
      @recurrance = "Weekly"
    elsif self.recurring == 2
      @recurrance = "Monthly"
    end
  end

end
